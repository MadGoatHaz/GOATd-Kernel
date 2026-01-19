use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;
use libc::{pipe, sched_setaffinity, cpu_set_t, CPU_ZERO, CPU_SET};

const NUM_EXCHANGES: u32 = 100_000;

fn main() {
    println!("=== Standalone Efficiency Sanity Test ===");
    println!("Measuring raw kernel context-switch RTT via pipe ping-pong");
    println!("Exchanges: {}", NUM_EXCHANGES);
    println!();

    // Create two pipes: pipe1 (A->B) and pipe2 (B->A)
    let mut pipe1_fds = [0; 2];
    let mut pipe2_fds = [0; 2];

    unsafe {
        if pipe(pipe1_fds.as_mut_ptr()) != 0 {
            eprintln!("Failed to create pipe1");
            std::process::exit(1);
        }
        if pipe(pipe2_fds.as_mut_ptr()) != 0 {
            eprintln!("Failed to create pipe2");
            std::process::exit(1);
        }
    }

    let barrier = Arc::new(Barrier::new(3));

    // Thread A (runs on CPU 0)
    let barrier_a = barrier.clone();
    let pipe1_dup = (pipe1_fds[1], pipe1_fds[0]);
    let pipe2_dup = (pipe2_fds[1], pipe2_fds[0]);

    let thread_a = thread::spawn(move || {
        // Pin to CPU 0
        pin_to_cpu(0);
        barrier_a.wait();

        let write_fd = pipe1_dup.0;
        let read_fd = pipe2_dup.1;
        let mut buf = [0u8; 1];

        for _ in 0..NUM_EXCHANGES {
            // Write to pipe1 (signal to B)
            unsafe {
                libc::write(write_fd, &[0u8] as *const u8 as *const libc::c_void, 1);
            }
            // Read from pipe2 (wait for B's response)
            unsafe {
                libc::read(read_fd, &mut buf as *mut u8 as *mut libc::c_void, 1);
            }
        }
    });

    // Thread B (runs on CPU 1)
    let barrier_b = barrier.clone();
    let pipe1_dup = (pipe1_fds[1], pipe1_fds[0]);
    let pipe2_dup = (pipe2_fds[1], pipe2_fds[0]);

    let thread_b = thread::spawn(move || {
        // Pin to CPU 1
        pin_to_cpu(1);
        barrier_b.wait();

        let read_fd = pipe1_dup.1;
        let write_fd = pipe2_dup.0;
        let mut buf = [0u8; 1];

        for _ in 0..NUM_EXCHANGES {
            // Read from pipe1 (wait for A's signal)
            unsafe {
                libc::read(read_fd, &mut buf as *mut u8 as *mut libc::c_void, 1);
            }
            // Write to pipe2 (respond to A)
            unsafe {
                libc::write(write_fd, &[0u8] as *const u8 as *const libc::c_void, 1);
            }
        }
    });

    // Wait for barrier - both threads ready
    barrier.wait();
    let start = Instant::now();

    // Wait for both threads to complete
    thread_a.join().expect("Thread A panicked");
    thread_b.join().expect("Thread B panicked");

    let elapsed = start.elapsed();

    // Calculate average RTT
    // Each exchange is: A writes -> B reads -> B writes -> A reads (1 RTT)
    let total_microseconds = elapsed.as_micros() as f64;
    let average_rtt_us = total_microseconds / NUM_EXCHANGES as f64;

    println!("Total time: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
    println!("Average RTT: {:.2} µs", average_rtt_us);
    println!();
    println!("(Lower is better. Expected: ~8.4µs for reference)");
}

fn pin_to_cpu(cpu_id: usize) {
    unsafe {
        let mut set: cpu_set_t = std::mem::zeroed();
        CPU_ZERO(&mut set);
        CPU_SET(cpu_id, &mut set);

        let result = sched_setaffinity(0, std::mem::size_of::<cpu_set_t>(), &set);
        if result != 0 {
            eprintln!("Warning: Failed to pin thread to CPU {}", cpu_id);
        }
    }
}
