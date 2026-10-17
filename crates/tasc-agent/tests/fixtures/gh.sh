#!/bin/sh
if [ "$GH_HOST" != "github.com" ] || [ "$GH_PROMPT_DISABLED" != "1" ]; then
  printf 'host or prompt boundary not enforced\n' >&2
  exit 1
fi
read -r mode < "$0.mode"
printf '%s\n' "$*" >> "$0.calls"
case "$1 $2" in
  "version ")
    if [ "$mode" = "bad-version" ]; then printf 'unexpected\n'; else printf 'gh version 2.101.0\n'; fi
    exit 0 ;;
  "skill --help")
    if [ "$mode" = "unavailable" ]; then printf 'unknown command skill\n' >&2; exit 1; fi
    printf 'search preview\n'
    exit 0 ;;
  "skill search")
    if [ "$3" = "--help" ]; then
      if [ "$mode" = "no-search" ]; then printf 'unknown command search\n' >&2; exit 1; fi
      if [ "$mode" = "no-json" ]; then printf 'search <query>\n'; else printf 'search <query> --json --limit repo skillName namespace description stars path\n'; fi
      exit 0
    fi ;;
  "skill preview")
    if [ "$3" = "--help" ]; then
      if [ "$mode" = "no-preview" ]; then printf 'unknown command preview\n' >&2; exit 1; fi
      printf 'preview <repository> [<skill>]\n'
      exit 0
    fi ;;
  "skill install")
    if [ "$3" = "--help" ]; then
      printf 'install <repository> [<skill>] --dir --pin --force\n'
      exit 0
    fi
    if [ "$mode" = "timeout" ]; then exec /bin/sleep 10; fi
    shift 2
    target=""
    while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
      case "$1" in
        --dir) target="$2"; shift 2 ;;
        --pin) shift 2 ;;
        --force) shift ;;
        *) printf 'unexpected install flag\n' >&2; exit 1 ;;
      esac
    done
    shift
    repository="$1"
    source_path="$2"
    name="${source_path%/SKILL.md}"
    name="${name#skills/}"
    name="${name#*/}"
    /bin/mkdir -p "$target/$name"
    /bin/cat > "$target/$name/SKILL.md" <<EOF
---
name: ${name##*/}
description: Fixture skill
metadata:
  github-repo: https://github.com/$repository
  github-ref: refs/heads/main
  github-tree-sha: tree-install
  github-path: $source_path
---
Fixture content.
EOF
    if [ "$mode" = "partial-failure" ]; then
      printf 'partial install failed\n' >&2
      exit 1
    fi
    printf 'Installed %s\n' "${name##*/}"
    exit 0 ;;
  "skill update")
    if [ "$3" = "--help" ]; then
      printf 'update [<skill>...] --dir --dry-run --all --force\n'
      exit 0
    fi
    shift 2
    target=""
    dry_run=0
    while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
      case "$1" in
        --dir) target="$2"; shift 2 ;;
        --dry-run) dry_run=1; shift ;;
        --all|--force) shift ;;
        *) printf 'unexpected update flag\n' >&2; exit 1 ;;
      esac
    done
    shift
    name="$1"
    if [ "$dry_run" = "1" ]; then
      if [ "$mode" = "outdated" ] || [ "$mode" = "update-fail" ]; then
        printf '  • %s tree-old > tree-new\n' "$name"
      fi
      exit 0
    fi
    if [ "$mode" = "timeout" ]; then exec /bin/sleep 10; fi
    if [ "$mode" = "update-fail" ]; then
      printf 'partial update failed\n' >&2
      exit 1
    fi
    if [ "$mode" = "outdated" ] || [ "$mode" = "supported" ]; then
      skill_dir="$target/$name"
      if [ ! -d "$skill_dir" ]; then
        skill_dir="$target/$(/usr/bin/basename "$name")"
      fi
      /bin/cat > "$skill_dir/SKILL.md" <<EOF
---
name: $(/usr/bin/basename "$name")
description: Updated fixture skill
metadata:
  github-repo: https://github.com/alpha/skills
  github-ref: refs/heads/main
  github-tree-sha: tree-updated
  github-path: skills/$(/usr/bin/basename "$name")/SKILL.md
---
Updated fixture content.
EOF
      printf 'Updated %s\n' "$name"
      exit 0
    fi
    printf 'update failed\n' >&2
    exit 1 ;;
  *) printf 'unexpected invocation\n' >&2; exit 1 ;;
esac
case "$mode" in
  auth) printf 'Run gh auth login\n' >&2; exit 4 ;;
  auth-env) printf 'set GH_TOKEN\n' >&2; exit 4 ;;
  forbidden) printf 'HTTP 403: 403 Forbidden\n' >&2; exit 1 ;;
  fail) printf 'network failed\n' >&2; exit 1 ;;
  timeout) exec /bin/sleep 10 ;;
  malformed) printf '{broken'; exit 0 ;;
  invalid-utf8) printf '\377'; exit 0 ;;
  oversized) exec /usr/bin/head -c 1048577 /dev/zero ;;
  oversized-stderr) /usr/bin/head -c 1048577 /dev/zero >&2; exit 1 ;;
esac
if [ "$2" = "search" ]; then
  /bin/cat "$0.search.json"
else
  printf 'Preview only: upstream instructions, not executed.\n'
fi
