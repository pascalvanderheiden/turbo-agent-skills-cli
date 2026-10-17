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
