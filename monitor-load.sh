#!/bin/bash

clear

THREADS=$(nproc)
CPU_CAPACITY=$((THREADS * 100))

# Node test is not measured here; default to 0 so COMBINED math stays clean
NODE_CPU=${NODE_CPU:-0}
NODE_RSS_MB=${NODE_RSS_MB:-0}

# Measure real WSL CPU usage from /proc/stat + per-process PicoLimbo CPU over 0.5s
read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat
TOTAL1=$((user + nice + system + idle + iowait + irq + softirq + steal))
IDLE1=$((idle + iowait))

PIDS=$(pgrep pico_limbo)
declare -A T1
for pid in $PIDS; do
  read utime stime < <(awk '{print $14, $15}' /proc/$pid/stat 2>/dev/null)
  [ -n "$utime" ] && T1[$pid]=$((utime + stime))
done

sleep 0.5

read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat
TOTAL2=$((user + nice + system + idle + iowait + irq + softirq + steal))
IDLE2=$((idle + iowait))

declare -A T2
for pid in $PIDS; do
  read utime stime < <(awk '{print $14, $15}' /proc/$pid/stat 2>/dev/null)
  [ -n "$utime" ] && T2[$pid]=$((utime + stime))
done

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
PICO_COUNT=0
PICO_CPU="0.00"
PICO_RSS_MB="0.00"
PICO_TABLE=""

# Per-process CPU% = (proc jiffy delta / system jiffy delta) * 100 * threads
for pid in $PIDS; do
  [ -n "${T2[$pid]}" ] || continue
  PICO_COUNT=$((PICO_COUNT + 1))

  DIFF=$(( ${T2[$pid]:-0} - ${T1[$pid]:-0} ))
  CPU=$(awk -v d="$DIFF" -v total="$TOTAL_DIFF" -v n="$THREADS" 'BEGIN { if (total > 0) printf "%.2f", (d/total)*100*n; else printf "0.00" }')

  read rss etime args < <(ps -p "$pid" -o rss=,etime=,args= 2>/dev/null)
  rss=${rss:-0}

  PICO_CPU=$(awk -v a="$PICO_CPU" -v b="$CPU" 'BEGIN {printf "%.2f", a+b}')
  PICO_RSS_MB=$(awk -v a="$PICO_RSS_MB" -v b="$rss" 'BEGIN {printf "%.2f", a + b/1024}')
  PICO_TABLE+=$(printf "%s\t%s\t%s\t%s\t%s\n" "$CPU" "$pid" "$rss" "$etime" "$args")
  PICO_TABLE+=$'\n'
done

PICO_AVG_RSS_MB=$(awk -v total="$PICO_RSS_MB" -v count="$PICO_COUNT" 'BEGIN {if (count > 0) printf "%.2f", total/count; else printf "0.00"}')

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
printf "%-7s %-7s %-9s %-12s %s\n" "%CPU" "PID" "RSS" "ELAPSED" "COMMAND"
printf "%b" "$PICO_TABLE" | grep -v '^$' | sort -t$'\t' -k1 -rn | head -15 | awk -F'\t' '{printf "%-7s %-7s %-9s %-12s %s\n", $1, $2, $3, $4, $5}'
echo

echo "==================== LISTENING LIMBO PORTS ==========="
ss -tulpen 2>/dev/null | grep pico_limbo | head -20
