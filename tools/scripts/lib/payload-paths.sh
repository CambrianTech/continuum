# The same deployment-path record as paths::payload_root and payload-paths.ps1.
managed_payload_root() {
  local home="${1:-${CONTINUUM_HOME:-$HOME/.continuum}}" root record
  record="$home/payload-root"
  if [ ! -e "$record" ]; then printf '%s\n' "$home"; return; fi
  root="$(cat "$record")" || return 1
  root="${root%$'\r'}"
  case "$root" in
    /*) ;;
    [A-Za-z]:[\\/]*|\\\\*)
      command -v cygpath >/dev/null 2>&1 || { echo "$record contains a foreign platform path" >&2; return 1; }
      root="$(cygpath -u "$root")" || return 1;;
    *) echo "$record must name one absolute payload directory" >&2; return 1;;
  esac
  case "$root" in *$'\n'*|*$'\r'*) echo "$record contains multiple paths" >&2; return 1;; esac
  [ -d "$root" ] || { echo "Selected payload directory $root is unavailable" >&2; return 1; }
  printf '%s\n' "$root"
}

initialize_managed_payload_root() (
  local home="$1" cold="${2:-}" root record temporary entry identity scope native legacy=0
  mkdir -p "$home" || return 1
  home="$(cd "$home" && pwd -P)" || return 1
  record="$home/payload-root"
  if [ -e "$record" ]; then managed_payload_root "$home"; return; fi
  for entry in bin tools lib cuda-toolkit; do [ ! -e "$home/$entry" ] || legacy=1; done
  root="$home"
  if [ -n "$cold" ] && [ "$legacy" = 0 ]; then
    identity="$home"
    if command -v cygpath >/dev/null 2>&1; then identity="$(cygpath -w "$home")" || return 1; fi
    if command -v sha256sum >/dev/null 2>&1; then
      scope="$(printf '%s' "$identity" | sha256sum)" || return 1
    else
      scope="$(printf '%s' "$identity" | shasum -a 256)" || return 1
    fi
    scope="${scope:0:16}"
    root="$cold/payloads/$scope"
    if [ -d "$root" ] && [ -n "$(find "$root" -mindepth 1 -maxdepth 1 -print -quit)" ]; then
      echo "Unowned payload directory $root is not empty; refusing to adopt another install" >&2; return 1
    fi
  fi
  mkdir -p "$home" "$root" || return 1
  root="$(cd "$root" && pwd -P)" || return 1
  temporary="$(mktemp "$record.XXXXXX")" || return 1
  trap 'rm -f -- "$temporary"' EXIT
  native="$root"
  if command -v cygpath >/dev/null 2>&1; then native="$(cygpath -w "$root")" || return 1; fi
  printf '%s\n' "$native" > "$temporary" || return 1
  ln "$temporary" "$record" || return 1
  printf '%s\n' "$root"
)
