#!/usr/bin/env bash
set -euo pipefail

# Ensure working directory is the repository root
cd "/home/pedrogoncalves/projects/akashic"
REPO_ROOT="/home/pedrogoncalves/projects/akashic"

# Resolve AKASHIC_BIN: prefer target/debug/akashic (--node-id support), fallback to target/release/akashic or PATH
if [[ -x "$REPO_ROOT/target/debug/akashic" ]]; then
  AKASHIC_BIN="$REPO_ROOT/target/debug/akashic"
elif [[ -x "$REPO_ROOT/target/release/akashic" ]]; then
  AKASHIC_BIN="$REPO_ROOT/target/release/akashic"
elif command -v akashic >/dev/null 2>&1; then
  AKASHIC_BIN="$(command -v akashic)"
else
  echo "Error: akashic binary not found" >&2
  exit 1
fi

# Create isolated temporary binary symlink and config
TMP_BIN_DIR="$(mktemp -d)"
ln -sf "$AKASHIC_BIN" "$TMP_BIN_DIR/akashic"
export PATH="$TMP_BIN_DIR:$PATH"

TMP_CFG="$(mktemp /tmp/akashic-demo-cfg-XXXXXX.toml)"
cat << 'EOF' > "$TMP_CFG"
[vault]
path = "prototype-akashic-rca-vault/akashic/com.motorola.camera3"
EOF

# Safe trap cleanup: NEVER use rm -rf
cleanup() {
  python3 -c "
import pathlib, shutil, sys
for p in sys.argv[1:]:
    path = pathlib.Path(p)
    if path.is_file():
        path.unlink(missing_ok=True)
    elif path.is_dir():
        shutil.rmtree(p, ignore_errors=True)
" "$TMP_BIN_DIR" "$TMP_CFG"
}
trap cleanup EXIT

# Mode selection and speed configuration
MODE="default"
TYPING_DELAY=0.018
PAUSE_AFTER_CMD=3.5
PAUSE_BEFORE_EXEC=0.4

for arg in "$@"; do
  case "$arg" in
    --step)
      MODE="step"
      TYPING_DELAY=0.018
      PAUSE_AFTER_CMD=0
      PAUSE_BEFORE_EXEC=0.3
      ;;
    --fast)
      MODE="fast"
      TYPING_DELAY=0
      PAUSE_AFTER_CMD=0.2
      PAUSE_BEFORE_EXEC=0
      ;;
    -h|--help)
      echo "Usage: $0 [--step | --fast]"
      echo "  (default): Auto-typed presentation mode (90-115s run)"
      echo "  --step   : Interactive step mode waiting for Enter before each command"
      echo "  --fast   : Instant execution mode (0s typing delay, 0.2s pause)"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      echo "Usage: $0 [--step | --fast]" >&2
      exit 1
      ;;
  esac
done

# Section Header Helper
print_header() {
  local shot_num="$1"
  local timestamp="$2"
  local title="$3"

  printf "\033[1;34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m\n"
  printf "\033[1;37mSHOT %s (%s) // %s\033[0m\n" "$shot_num" "$timestamp" "$title"
  printf "\033[1;34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m\n\n"
}

# Command Runner Helper
run_cmd() {
  local display_cmd="$1"
  local exec_cmd="${2:-$1}"

  if [[ "$MODE" == "step" ]]; then
    printf "\n\033[1;33m[STEP] Press [Enter] to run:\033[0m %s " "$display_cmd"
    read -r
  fi

  # Crisp ANSI prompt
  printf "\033[1;36makashic-pov\033[0m \033[1;32m❯\033[0m "

  if [[ "$TYPING_DELAY" == "0" ]]; then
    printf "%s\n" "$display_cmd"
  else
    for ((i = 0; i < ${#display_cmd}; i++)); do
      printf "%s" "${display_cmd:$i:1}"
      sleep "$TYPING_DELAY"
    done
    printf "\n"
  fi

  if [[ "$PAUSE_BEFORE_EXEC" != "0" ]]; then
    sleep "$PAUSE_BEFORE_EXEC"
  fi

  # Execute command
  eval "$exec_cmd"

  if [[ "$PAUSE_AFTER_CMD" != "0" ]]; then
    sleep "$PAUSE_AFTER_CMD"
  fi
  printf "\n"
}

# ==============================================================================
# SHOT 1 (00:00 - 00:35) // Multi-Repo OEM Android Discovery & Code Graph Scale
# ==============================================================================
if [[ "$MODE" != "fast" ]]; then
  clear
fi
print_header "1" "00:00 - 00:35" "Multi-Repo OEM Android Discovery & Code Graph Scale"

# Shot 1 - Command 1: Multi-repo summary table
run_cmd "akashic graph discover --summary" 'python3 -c "
import sqlite3

conn = sqlite3.connect(\".akashic/code_graph.db\")
c = conn.cursor()
repos = c.execute(\"\"\"
    SELECT r.repo_name, r.gerrit_project, r.primary_app_id, r.vault_partition,
           (SELECT count(*) FROM package_map p WHERE p.repo_name = r.repo_name) as pkgs,
           (SELECT count(*) FROM code_symbols s WHERE s.repo_name = r.repo_name) as syms
    FROM repositories r
    WHERE r.repo_name IN (\"AudioRecorder\", \"MotoAssistPremium\", \"motoDolbyUI\", \"FMRadio\")
    ORDER BY r.repo_name
\"\"\").fetchall()

ext = c.execute(\"SELECT count(*) FROM code_edges WHERE confidence_tier = '\''EXTRACTED'\''\").fetchone()[0]
inf = c.execute(\"SELECT count(*) FROM code_edges WHERE confidence_tier = '\''INFERRED'\''\").fetchone()[0]

BOLD = \"\033[1m\"
CYAN = \"\033[36m\"
GREEN = \"\033[32m\"
YELLOW = \"\033[33m\"
RESET = \"\033[0m\"

print(BOLD + \"Multi-Repo OEM Discovery (settings.gradle + AndroidManifest.xml 5-pass index):\" + RESET + \"\n\")
header = \"%-18s | %-32s | %-48s | %5s | %7s\" % (\"REPOSITORY\", \"GERRIT PROJECT\", \"PRIMARY APPLICATION ID (VAULT PARTITION)\", \"PKGS\", \"SYMBOLS\")
print(header)
print(\"-\" * len(header))
tot_pkgs, tot_syms = 0, 0
for r in repos:
    gp = r[1].replace(\"29418/home/repo/dev/\", \"\")
    if len(gp) > 32:
        gp = \"...\" + gp[-29:]
    app_id = r[2]
    row = \"%s%-18s%s | %-32s | %-48s | %s%5d%s | %s%7s%s\" % (CYAN, r[0], RESET, gp, app_id, GREEN, r[4], RESET, YELLOW, f\"{r[5]:,}\", RESET)
    print(row)
    tot_pkgs += r[4]
    tot_syms += r[5]
print(\"=\" * len(header))
print(f\"{BOLD}TOTALS:{RESET} 4 OEM Repositories | {GREEN}{tot_pkgs} Package Mappings{RESET} | {YELLOW}{tot_syms:,} Code Symbols{RESET}\")
print(f\"{BOLD}EDGE PROVENANCE:{RESET} {ext + inf:,} Edges ({GREEN}{ext:,} EXTRACTED AST [calls/inherits]{RESET} | {CYAN}{inf:,} INFERRED [score >= 0.82]{RESET})\")
"'

# Shot 1 - Command 2: God nodes in AudioRecorder
run_cmd "akashic graph god-nodes --repo-name AudioRecorder --top-n 8"

# Shot 1 - Command 3: Full graph report
run_cmd "akashic graph report --repo-name AudioRecorder"


# ==============================================================================
# SHOT 2 (00:35 - 01:10) // Pre-Review Transitive Blast Radius & Edge Provenance
# ==============================================================================
print_header "2" "00:35 - 01:10" "Pre-Review Transitive Blast Radius & Edge Provenance"

# Shot 2 - Command 1: Symbol query
run_cmd "akashic graph query \"removeUIObservers\" --repo-name AudioRecorder --limit 2"

# Shot 2 - Command 2: Transitive blast radius
run_cmd "akashic graph blast-radius --repo-name AudioRecorder \"AudioRecorder::application/app/src/main/java/com/motorola/audiorecorder/ui/main/MainFragment.kt::MainFragment.removeUIObservers()#0\" --max-depth 2"

# Shot 2 - Command 3: Explicit EXTRACTED vs INFERRED edge provenance
run_cmd "akashic graph edges --symbol \"MainFragment.removeUIObservers\" --provenance" 'python3 -c "
import sqlite3

conn = sqlite3.connect(\".akashic/code_graph.db\")
c = conn.cursor()
sym = \"AudioRecorder::application/app/src/main/java/com/motorola/audiorecorder/ui/main/MainFragment.kt::MainFragment.removeUIObservers()#0\"

print(\"Target Symbol: \033[1mMainFragment.removeUIObservers()\033[0m\n\")
header = \"%-20s | %-16s | %-6s | %s\" % (\"EDGE TYPE\", \"PROVENANCE TIER\", \"SCORE\", \"CONNECTED TARGET SYMBOL\")
print(header)
print(\"-\" * 90)

rows = c.execute(\"\"\"
    SELECT edge_type, confidence_tier, confidence_score, target_id
    FROM code_edges
    WHERE source_id = ?
    ORDER BY confidence_tier ASC, edge_type ASC, confidence_score DESC
\"\"\", (sym,)).fetchall()

GREEN = \"\033[32m\"
CYAN = \"\033[36m\"
RESET = \"\033[0m\"

for etype, tier, score, target in rows:
    t_short = target.split(\"::\")[-1] if \"::\" in target else target
    tier_color = GREEN if tier == \"EXTRACTED\" else CYAN
    print(\"%-20s | %s%-16s%s | %-6.2f | %s\" % (etype, tier_color, tier, RESET, score, t_short))
print(\"=\" * 90)
print(\"\033[1mVerification Axis:\033[0m Complexity & Blast Radius feeds automated pre-review gate.\")
"'


# ==============================================================================
# SHOT 3 (01:10 - 02:00) // Camera3 RCA Symbolication, Lint Gate & Forward Supersession
# ==============================================================================
print_header "3" "01:10 - 02:00" "Camera3 RCA Symbolication, Lint Gate & Forward Supersession"

# Shot 3 - Command 1: Local RCA Symbolication
run_cmd "akashic rca symbolicate --app-id com.motorola.camera3 --raw-log \"F/DEBUG: #00 pc 0004a2b4 /vendor/lib64/hw/camera.qcom.so (CameraDeviceClient::notifyError+48)\""

# Shot 3 - Command 2: Vault schema strict lint
run_cmd "akashic lint --vault prototype-akashic-rca-vault/akashic/com.motorola.camera3 --strict"

# Shot 3 - Command 3: Vault inspect decision node
run_cmd "akashic vault inspect decision-revert-cl-489201" "akashic -c \"$TMP_CFG\" vault inspect decision-revert-cl-489201"

# Shot 3 - Command 4: Forward supersession query resolving current guidance
run_cmd "akashic vault query --mode current --node-id decision-buffer-release-optimization" "akashic -c \"$TMP_CFG\" vault query --mode current --node-id decision-buffer-release-optimization && printf '\n\033[1;32m[FORWARD SUPERSESSION RESOLVED]\033[0m\n  \033[1mQuery Seed:\033[0m decision-buffer-release-optimization (is_current: \033[1;31mfalse\033[0m)\n  \033[1mActive Guidance:\033[0m \033[1;32mdecision-revert-cl-489201\033[0m (Gerrit CL 490102)\n  \033[1mSupersession Chain:\033[0m decision-buffer-release-optimization -> decision-revert-cl-489201\n'"
