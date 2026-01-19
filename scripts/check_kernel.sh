#!/bin/bash
# GOATd Kernel Auditor v9 - Static Config Focused

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${CYAN}--- GOATd Kernel Auditor (Deep Audit) ---${NC}"

# --- CONFIG SEARCH LOGIC ---
# We hunt for the actual build config to see what the kernel is SET to.
KVER=$(uname -r)
CONFIG_FILE=""

if [ -f /proc/config.gz ]; then
    CONFIG_FILE="/proc/config.gz"
    CAT_CMD="zcat"
elif [ -f "/boot/config-$KVER" ]; then
    CONFIG_FILE="/boot/config-$KVER"
    CAT_CMD="cat"
elif [ -f "/lib/modules/$KVER/build/.config" ]; then
    CONFIG_FILE="/lib/modules/$KVER/build/.config"
    CAT_CMD="cat"
fi

# 1. HZ DETECTION (Set vs Measured)
# SET HZ
if [ -n "$CONFIG_FILE" ]; then
    HZ_SET=$($CAT_CMD "$CONFIG_FILE" | grep "^CONFIG_HZ=" | cut -d= -f2)
else
    HZ_SET="Unknown (IKCONFIG Missing)"
fi

# MEASURED HZ (Kept as a reference for Tickless behavior)
S1=$(grep "timer" /proc/interrupts | awk '{for(i=2;i<=NF-2;i++) sum+=$i} END {print sum}')
sleep 0.5
S2=$(grep "timer" /proc/interrupts | awk '{for(i=2;i<=NF-2;i++) sum+=$i} END {print sum}')
HZ_MEASURED=$(( (S2 - S1) * 2 / $(nproc) ))

echo -e "Timer Frequency:  ${GREEN}${HZ_SET} Hz${NC} (Set) | ${YELLOW}${HZ_MEASURED} Hz${NC} (Measured)"

# 2. LTO STRATEGY
if [ -n "$CONFIG_FILE" ]; then
    if $CAT_CMD "$CONFIG_FILE" | grep -q "CONFIG_LTO_CLANG_THIN=y"; then LTO="Thin";
    elif $CAT_CMD "$CONFIG_FILE" | grep -q "CONFIG_LTO_CLANG_FULL=y"; then LTO="Full";
    else LTO="None"; fi
else
    # Fallback to binary check if config is missing
    [[ "$(uname -v)" == *"LLVM"* ]] && LTO="Active (Likely Thin/Full)" || LTO="None"
fi
echo -e "LTO Strategy:     ${GREEN}$LTO${NC}"

# 3. CPU SCHEDULER (SCX / LAVD Mode)
if [ -d /sys/kernel/sched_ext ]; then
    SCX_NAME=$(cat /sys/kernel/sched_ext/root/ops 2>/dev/null | awk '{print $1}')
    if command -v scxctl &> /dev/null; then
        SCX_MODE=$(scxctl get 2>/dev/null | grep -iE "Mode:|Profile:" | awk -F': ' '{print $2}')
    fi
    echo -e "CPU Scheduler:    ${GREEN}SCX ($SCX_NAME)${NC} [Mode: ${SCX_MODE:-Auto}]"
else
    [ -f /sys/kernel/debug/sched/bore ] && S="BORE" || S="EEVDF/CFS"
    echo -e "CPU Scheduler:    ${GREEN}$S${NC}"
fi

# 4. MGLRU STATUS
if [ -f /sys/kernel/mm/lru_gen/enabled ]; then
    MGL_VAL=$(cat /sys/kernel/mm/lru_gen/enabled)
    if [[ "$MGL_VAL" == "0x0007" ]]; then
        echo -e "MGLRU Status:     ${GREEN}FULL (0x0007)${NC}"
    else
        echo -e "MGLRU Status:     ${RED}PARTIAL ($MGL_VAL)${NC}"
    fi
else
    echo -e "MGLRU Status:     ${RED}Disabled${NC}"
fi

# 5. MODULE COUNT
MOD_COUNT=$(lsmod | wc -l)
echo -e "Loaded Modules:   ${GREEN}$((MOD_COUNT - 1))${NC}"

echo -e "${CYAN}------------------------------------------${NC}"
