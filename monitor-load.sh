#!/bin/bash

clear

THREADS=$(nproc)
CPU_CAPACITY=$((THREADS * 100))

# Measure real WSL CPU usage from /proc/stat over 0.5s
read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat
TOTAL1=$((user + nice + system + idle + iowait + irq + softirq + steal))
IDLE1=$((idle + iowait))

sleep 0.5

read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat
TOTAL2=$((user + nice + system + idle + iowait + irq + softirq + steal))
IDLE2=$((idle + iowait))

TOTAL_DIFF=$((TOTAL2 - TOTAL1))
IDLE_DIFF=$((IDLE2 - IDLE1))

if [ "$TOTAL_DIFF" -gt 0 ]; then
  WSL_CPU_USED=$(awk -v total="$TOTAL_DIFF" -v idle="$IDLE_DIFF" 'BEGIN { printf "%.2f", (100 * (total - idle) / total) }')
else
  WSL_CPU_USED="0.00"
fi

echo "==================== SYSTEM / WSL ===================="
echo "Logical CPU threads      : $THREADS"
echo "Total CPU capacity       : ${CPU_CAPACITY}%"
echo "WSL total CPU used       : ${WSL_CPU_USED}% of available WSL CPU"
echo

echo "==================== PICOLIMBO TOTAL ================="
PICO_COUNT=$(pgrep -c pico_limbo 2>/dev/null || echo 0)

PICO_CPU=$(ps -C pico_limbo -o %cpu= 2>/dev/null | awk '{sum+=$1} END {printf "%.2f", sum+0}')
PICO_RSS_MB=$(ps -C pico_limbo -o rss= 2>/dev/null | awk '{sum+=$1} END {printf "%.2f", sum/1024}')
PICO_AVG_RSS_MB=$(ps -C pico_limbo -o rss= 2>/dev/null | awk -v count="$PICO_COUNT" '{sum+=$1} END {if (count > 0) printf "%.2f", (sum/1024)/count; else printf "0.00"}')

echo "PicoLimbo processes      : $PICO_COUNT"
echo "PicoLimbo total CPU      : ${PICO_CPU}% / ${CPU_CAPACITY}% capacity"
echo "PicoLimbo total RAM      : ${PICO_RSS_MB} MB"
echo "PicoLimbo avg RAM/process: ${PICO_AVG_RSS_MB} MB"
echo

echo "==================== COMBINED TEST TOTAL ============="
COMBINED_CPU=$(awk -v a="$PICO_CPU" -v b="$NODE_CPU" 'BEGIN {printf "%.2f", a+b}')
COMBINED_RAM=$(awk -v a="$PICO_RSS_MB" -v b="$NODE_RSS_MB" 'BEGIN {printf "%.2f", a+b}')
COMBINED_CPU_THREADS=$(awk -v cpu="$COMBINED_CPU" 'BEGIN {printf "%.2f", cpu/100}')
COMBINED_CPU_TOTAL_PERCENT=$(awk -v cpu="$COMBINED_CPU" -v cap="$CPU_CAPACITY" 'BEGIN {printf "%.2f", (cpu/cap)*100}')

echo "Available CPU threads    : $THREADS"
echo "Total CPU capacity       : ${CPU_CAPACITY}%"
echo "CPU used                 : ${COMBINED_CPU}%"
echo "CPU used readable        : ${COMBINED_CPU_THREADS} / ${THREADS} logical threads"
echo "Total CPU usage          : ${COMBINED_CPU_TOTAL_PERCENT}% of available CPU"
echo "PicoLimbo     : ${COMBINED_RAM} MB"
echo

echo "==================== TOP PICOLIMBO BY CPU ============"
ps -C pico_limbo -o pid,%cpu,rss,vsz,etime,args --sort=-%cpu 2>/dev/null | head -15
echo

echo "==================== LISTENING LIMBO PORTS ==========="
ss -tulpen 2>/dev/null | grep pico_limbo | head -20
