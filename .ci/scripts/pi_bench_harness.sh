#!/usr/bin/env bash
# Pi hotpath bench harness — runs the `hotpath_e2e` bench on a Raspberry Pi and captures
# SPEED + MEMORY + CPU-UTILIZATION + POWER-PROXIES in one shot: a per-sample CSV plus a summary.
#
# Runs ON THE PI (reads /proc + /sys + vcgencmd). Cross-build the bench on an x86 host for
# aarch64-unknown-linux-musl (the shipping `profiling` profile — opt-level 3; see the root
# Cargo.toml), scp the ELF over, then:
#
#   ./pi_bench_harness.sh <path-to-hotpath_e2e-binary> [label] [iters]
#
# - SPEED comes from the bench's own JSON report (run_cycle / emit_reading /
#   publish_rendered_tapped percentiles).
# - MEMORY is the process peak RSS (VmHWM) plus the bench's own alloc total.
# - CPU UTILIZATION is system-wide, aggregate + per-core (from /proc/stat), to show the workload
#   leaves cores free for other processes.
# - POWER on a Pi Zero 2 W has no internal current sensor, so we log PROXIES (CPU freq, SoC temp,
#   throttle flags). For real watts add an external INA219/INA260 on the 5 V rail (I2C enabled:
#   `dtparam=i2c_arm=on` + reboot) and print "watts=<float>" from a companion reader — the CSV has
#   a `watts` column ready.
#
# The sampler is a SINGLE python process pinned to CPU 0 (one process, direct /proc reads — no
# per-sample fork storm) and the bench is pinned to CPUs 1..N so the polling does not skew the
# workload's tail latencies. `p50` is the stable reference.
set -euo pipefail
export LC_ALL=C

BIN="${1:?usage: pi_bench_harness.sh <hotpath_e2e binary> [label] [iters]}"
LABEL="${2:-pi-bench}"
ITERS="${3:-${HOTPATH_ITERS:-500000}}"
OUT="${BENCH_OUT_DIR:-/tmp/pi-bench}"; mkdir -p "$OUT"
SAMP="$OUT/${LABEL}.samples.csv"; JSON="$OUT/${LABEL}.bench.json"; RUNLOG="$OUT/${LABEL}.run.out"
chmod +x "$BIN" 2>/dev/null || true
ncpu=$(nproc)

gov=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo "?")
echo "== $LABEL == $(uname -m) ${ncpu} cores, gov=$gov"
echo "baseline: temp=$(vcgencmd measure_temp 2>/dev/null) volt=$(vcgencmd measure_volts 2>/dev/null) throttled=$(vcgencmd get_throttled 2>/dev/null)"
echo "iters=$ITERS (~$(awk "BEGIN{printf \"%.0f\", $ITERS/1600}")s at ~1.6k cycles/s)"

# Bench on CPUs 1..N-1 (sampler gets CPU 0). Single core if that's all we have.
[ "$ncpu" -gt 1 ] && bench_cpus="1-$((ncpu-1))" || bench_cpus="0"
HOTPATH_ITERS="$ITERS" HOTPATH_OUTPUT_FORMAT=json HOTPATH_OUTPUT_PATH="$JSON" \
  taskset -c "$bench_cpus" "$BIN" >"$RUNLOG" 2>&1 &
PID=$!
start=$(date +%s%3N)

# --- lightweight single-process sampler (python, pinned CPU 0) ---
taskset -c 0 python3 - "$PID" "$SAMP" "$ncpu" <<'PY' &
import os, sys, time
pid, csv_path, ncpu = int(sys.argv[1]), sys.argv[2], int(sys.argv[3])
FREQ="/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"; TEMP="/sys/class/thermal/thermal_zone0/temp"
def rd(p):
    try:
        with open(p) as f: return f.read()
    except: return ""
def cpu_snap():
    d={}
    for ln in rd("/proc/stat").splitlines():
        if ln.startswith("cpu"):
            p=ln.split(); k=p[0]; v=[int(x) for x in p[1:]]
            d[k]=(v[3]+v[4], sum(v))   # (idle, total)
    return d
def rss(pid):
    for ln in rd(f"/proc/{pid}/status").splitlines():
        if ln.startswith("VmHWM"): return int(ln.split()[1])
    return 0
cols=["t_ms","freq_khz","temp_mC","rss_kb","cpu_pct"]+[f"cpu{c}_pct" for c in range(ncpu)]+["watts"]
f=open(csv_path,"w"); f.write(",".join(cols)+"\n")
prev=cpu_snap(); t0=time.time()
while os.path.exists(f"/proc/{pid}"):
    time.sleep(0.2)
    cur=cpu_snap()
    def pct(k):
        di=cur[k][0]-prev[k][0]; dt=cur[k][1]-prev[k][1]
        return 100.0*(1-di/dt) if dt>0 else 0.0
    agg=pct("cpu"); per=[pct(f"cpu{c}") for c in range(ncpu)]
    prev=cur
    row=[str(int((time.time()-t0)*1000)), rd(FREQ).strip() or "0", rd(TEMP).strip() or "0",
         str(rss(pid)), f"{agg:.1f}"]+[f"{p:.0f}" for p in per]+[""]
    f.write(",".join(row)+"\n"); f.flush()
f.close()
PY
SAMPLER=$!
wait "$PID" || true
elapsed_ms=$(( $(date +%s%3N) - start ))
wait "$SAMPLER" 2>/dev/null || true
final_throttled=$(vcgencmd get_throttled 2>/dev/null || echo NA)

echo
echo "=== RUN ==="
echo "  iters=$ITERS  wall=$(awk "BEGIN{printf \"%.1f\", $elapsed_ms/1000}")s  throughput=$(awk "BEGIN{printf \"%.0f\", $ITERS/($elapsed_ms/1000)}") cycles/s"

echo "=== SPEED (from $JSON) ==="
python3 - "$JSON" <<'PY'
import json, sys
try: d=json.load(open(sys.argv[1]))
except Exception as e: print("  (no json:", e, ")"); sys.exit(0)
want={"run_cycle","emit_reading","publish_rendered_tapped"}
print("  %-26s %10s %10s %10s %10s" % ("fn","avg","p50","p95","p99"))
for r in d.get("functions_timing",{}).get("data",[]):
    n=r["name"].split("::")[-1]
    if n in want: print("  %-26s %10s %10s %10s %10s" % (n,r.get("avg"),r.get("p50"),r.get("p95"),r.get("p99")))
PY

# --- CSV-driven summary (memory / cpu / power-proxies) ---
python3 - "$SAMP" "$ncpu" "$final_throttled" <<'PY'
import sys, csv
rows=list(csv.DictReader(open(sys.argv[1]))); ncpu=int(sys.argv[2]); thr=sys.argv[3]
if not rows: print("  (no samples)"); sys.exit(0)
peak_rss=max(int(r["rss_kb"]) for r in rows)
tmin=min(int(r["temp_mC"]) for r in rows); tmax=max(int(r["temp_mC"]) for r in rows)
fmax=max(int(r["freq_khz"]) for r in rows)
agg=[float(r["cpu_pct"]) for r in rows]
per=[[float(r[f"cpu{c}_pct"]) for r in rows] for c in range(ncpu)]
watts=[float(r["watts"]) for r in rows if r.get("watts","").strip()]
print("=== MEMORY ===")
print(f"  peak_RSS(VmHWM) = {peak_rss//1024} MB")
print("=== CPU UTILIZATION (system-wide, all %d cores; 100%% = all cores saturated) ===" % ncpu)
print(f"  overall avg={sum(agg)/len(agg):.1f}%  peak={max(agg):.1f}%")
print("  per-core avg: " + "  ".join(f"cpu{c}={sum(per[c])/len(per[c]):.0f}%" for c in range(ncpu)))
busy=sum(sum(per[c])/len(per[c]) for c in range(ncpu))/100.0
print(f"  => ~{busy:.1f} of {ncpu} cores busy on average; {ncpu-busy:.1f} cores free for other work")
print("=== POWER PROXIES ===")
print(f"  max_freq={fmax//1000} MHz   temp={tmin//1000}C -> {tmax//1000}C (rise {(tmax-tmin)//1000}C)   throttled_final={thr}")
print("  power(INA) avg=%.2fW peak=%.2fW" % (sum(watts)/len(watts), max(watts)) if watts else "  power(INA) = n/a (no sensor; add INA219/260 on the 5V rail)")
PY
echo "  samples: $SAMP"
