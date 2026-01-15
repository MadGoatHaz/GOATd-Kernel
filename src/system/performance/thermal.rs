//! CPU Thermal Data Reader
//!
//! Reads real-time core and package temperatures from Linux sysfs.
//! Supports multiple thermal data sources with fallback mechanisms.
//!
//! Supports:
//! - Intel coretemp driver (SYSFS_PLATFORM_CORETEMP_PATH)
//! - AMD k10temp driver (hwmon k10temp)
//! - Generic hwmon sensors (fallback)

use std::fs;
use std::path::Path;
use crate::hardware::cpu;

/// CPU thermal data snapshot
#[derive(Debug, Clone)]
pub struct ThermalData {
    /// Per-core temperatures in Celsius
    pub core_temperatures: Vec<f32>,
    /// Package/aggregate temperature in Celsius
    pub package_temperature: f32,
}

impl Default for ThermalData {
    fn default() -> Self {
        ThermalData {
            core_temperatures: Vec::new(),
            package_temperature: 0.0,
        }
    }
}

/// Read thermal data from Intel coretemp driver
fn read_coretemp() -> Option<ThermalData> {
    // coretemp is typically at /sys/devices/platform/coretemp.0/hwmon/hwmon*/
    let platform_dir = Path::new("/sys/devices/platform");
    
    if !platform_dir.exists() {
        return None;
    }
    
    let entries = fs::read_dir(platform_dir).ok()?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        
        let name = path.file_name()?.to_string_lossy().to_string();
        if !name.starts_with("coretemp.") {
            continue;
        }
        
        // Found a coretemp device, look for hwmon subdirectory
        let hwmon_dir = path.join("hwmon");
        if !hwmon_dir.exists() {
            continue;
        }
        
        if let Ok(hwmon_entries) = fs::read_dir(&hwmon_dir) {
            for hwmon_entry in hwmon_entries.flatten() {
                let hwmon_path = hwmon_entry.path();
                if hwmon_path.is_dir() {
                    if let Some(data) = read_coretemp_from_path(&hwmon_path) {
                        return Some(data);
                    }
                }
            }
        }
    }
    
    None
}

/// Read coretemp data from a specific hwmon path
fn read_coretemp_from_path(hwmon_path: &Path) -> Option<ThermalData> {
    let mut core_temps = Vec::new();
    let mut package_temp = 0.0;
    
    // Scan for all temp*_input files and their labels
    // Expected structure:
    // - temp1_input (Package temperature)
    // - temp2_input (Core 0), temp3_input (Core 1), etc.
    
    for temp_idx in 1..=255 {  // Support up to 255 temperature inputs
        let temp_path = hwmon_path.join(format!("temp{}_input", temp_idx));
        let label_path = hwmon_path.join(format!("temp{}_label", temp_idx));
        
        if !temp_path.exists() {
            // Stop when we hit the first missing temp file
            if temp_idx > 1 {
                break;
            }
            continue;
        }
        
        // Read the temperature value
        if let Ok(content) = fs::read_to_string(&temp_path) {
            if let Ok(millidegrees) = content.trim().parse::<f32>() {
                let celsius = millidegrees / 1000.0;
                
                // Sanity check: valid temperature range
                if celsius > 0.0 && celsius < 150.0 {
                    // Check label to determine if package or core
                    let is_package = if let Ok(label_content) = fs::read_to_string(&label_path) {
                        let label = label_content.trim();
                        // Only Package/Die are package temps. Core 0 is a physical core.
                        label.contains("Package") || label.contains("Die")
                    } else {
                        // Assume temp1 is package if no label
                        temp_idx == 1
                    };
                    
                    if is_package && package_temp == 0.0 {
                        package_temp = celsius;
                    } else if !is_package {
                        core_temps.push(celsius);
                    }
                }
            }
        }
    }
    
    // Return data only if we have core temperatures
    if !core_temps.is_empty() || package_temp > 0.0 {
        Some(ThermalData {
            core_temperatures: core_temps,
            package_temperature: package_temp,
        })
    } else {
        None
    }
}

/// Read thermal data from k10temp hwmon (AMD)
fn read_k10temp() -> Option<ThermalData> {
    // k10temp is typically at /sys/class/hwmon/hwmon3 on AMD Ryzen systems
    // but we should scan for it dynamically
    let hwmon_dir = Path::new("/sys/class/hwmon");
    
    if !hwmon_dir.exists() {
        return None;
    }
    
    let entries = fs::read_dir(hwmon_dir).ok()?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        
        let name_path = path.join("name");
        let name_content = fs::read_to_string(&name_path).ok()?;
        let name = name_content.trim();
        
        if name == "k10temp" {
            return read_k10temp_from_path(&path);
        }
    }
    
    None
}

/// Read k10temp data from a specific hwmon path
fn read_k10temp_from_path(hwmon_path: &Path) -> Option<ThermalData> {
    let mut sensor_temps = Vec::new();
    let mut package_temp = 0.0;
    
    // Read Tctl (package temperature) from temp1_input
    if let Ok(content) = fs::read_to_string(hwmon_path.join("temp1_input")) {
        if let Ok(millidegrees) = content.trim().parse::<f32>() {
            package_temp = millidegrees / 1000.0;
        }
    }
    
    // Read Tccd1, Tccd2, etc. (CCD sensors) from temp3_input, temp4_input, etc.
    let mut core_idx = 3; // Start from temp3 (temp3_input is Tccd1)
    loop {
        let temp_path = hwmon_path.join(format!("temp{}_input", core_idx));
        if temp_path.exists() {
            if let Ok(content) = fs::read_to_string(&temp_path) {
                if let Ok(millidegrees) = content.trim().parse::<f32>() {
                    let celsius = millidegrees / 1000.0;
                    // Sanity check: valid temperature range
                    if celsius > 0.0 && celsius < 150.0 {
                        sensor_temps.push(celsius);
                    }
                }
            }
            core_idx += 1;
        } else {
            break;
        }
    }
    
    if sensor_temps.is_empty() && package_temp > 0.0 {
        None // Only package temp available, not useful
    } else {
        Some(ThermalData {
            core_temperatures: sensor_temps,
            package_temperature: package_temp,
        })
    }
}

/// Read thermal data from generic hwmon sensors
fn read_generic_hwmon() -> Option<ThermalData> {
    let hwmon_dir = Path::new("/sys/class/hwmon");
    
    if !hwmon_dir.exists() {
        return None;
    }
    
    let entries = fs::read_dir(hwmon_dir).ok()?;
    let mut core_temps = Vec::new();
    let mut package_temp = 0.0;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        
        // Try to read temperature inputs
        let temp1_path = path.join("temp1_input");
        if temp1_path.exists() {
            if let Ok(content) = fs::read_to_string(&temp1_path) {
                if let Ok(millidegrees) = content.trim().parse::<f32>() {
                    let celsius = millidegrees / 1000.0;
                    if celsius > 0.0 && celsius < 150.0 {
                        package_temp = celsius;
                    }
                }
            }
        }
        
        // Try to read core temperatures (temp2, temp3, etc.)
        // Extended range to support high-core-count CPUs (up to 255 cores)
        for temp_idx in 2..=255 {
            let temp_path = path.join(format!("temp{}_input", temp_idx));
            if temp_path.exists() {
                if let Ok(content) = fs::read_to_string(&temp_path) {
                    if let Ok(millidegrees) = content.trim().parse::<f32>() {
                        let celsius = millidegrees / 1000.0;
                        if celsius > 0.0 && celsius < 150.0 {
                            core_temps.push(celsius);
                        }
                    }
                }
            } else if temp_idx > 2 {
                // Stop scanning if we hit a gap (no temp file)
                break;
            }
        }
        
        if !core_temps.is_empty() || package_temp > 0.0 {
            return Some(ThermalData {
                core_temperatures: core_temps,
                package_temperature: package_temp,
            });
        }
    }
    
    None
}

/// Replicate sensor data across physical cores
///
/// Maps sensor readings to physical cores. If we have fewer sensors
/// than cores (e.g., AMD k10temp with 2 CCD sensors for 16 cores),
/// evenly distributes sensors across cores.
fn map_sensors_to_cores(sensor_temps: Vec<f32>, physical_cores: u32) -> Vec<f32> {
    let sensor_count = sensor_temps.len() as u32;
    let core_count = physical_cores;
    
    // If we already have enough or no sensors, return as-is
    if sensor_count >= core_count || sensor_count == 0 {
        return sensor_temps;
    }
    
    // Replicate sensor data across cores
    let mut core_temps = vec![0.0; core_count as usize];
    let cores_per_sensor = core_count / sensor_count;
    
    for (sensor_idx, &temp) in sensor_temps.iter().enumerate() {
        let start_core = sensor_idx as u32 * cores_per_sensor;
        let end_core = if sensor_idx == sensor_temps.len() - 1 {
            core_count  // Last sensor gets remaining cores
        } else {
            (sensor_idx as u32 + 1) * cores_per_sensor
        };
        
        for core_idx in start_core..end_core {
            if (core_idx as usize) < core_temps.len() {
                core_temps[core_idx as usize] = temp;
            }
        }
    }
    
    core_temps
}

/// Read thermal data from the system with fallback mechanism
pub fn read_thermal_data() -> ThermalData {
    // Detect the actual physical core count
    let physical_cores = cpu::detect_cpu_cores().unwrap_or(1) as u32;
    
    // Try Intel coretemp first (Intel systems)
    if let Some(mut data) = read_coretemp() {
        // Map sensor data to physical cores if needed
        data.core_temperatures = map_sensors_to_cores(data.core_temperatures, physical_cores);
        return data;
    }
    
    // Try k10temp (AMD systems)
    if let Some(mut data) = read_k10temp() {
        // Map sensor data to physical cores if needed
        data.core_temperatures = map_sensors_to_cores(data.core_temperatures, physical_cores);
        return data;
    }
    
    // Fall back to generic hwmon
    if let Some(mut data) = read_generic_hwmon() {
        // Map sensor data to physical cores if needed
        data.core_temperatures = map_sensors_to_cores(data.core_temperatures, physical_cores);
        return data;
    }
    
    // Return default (no thermal data available)
    ThermalData::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_thermal_data_default() {
        let data = ThermalData::default();
        assert_eq!(data.core_temperatures.len(), 0);
        assert_eq!(data.package_temperature, 0.0);
    }
}
