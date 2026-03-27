# PetruTerm Shell Integration
# version: 1
# Tracks CWD, last command, and exit codes so Petrubot has context.
#
# Usage — add to ~/.zshrc:
#   source ~/.config/petruterm/shell-integration.zsh

_petruterm_context_file="${XDG_CACHE_HOME:-$HOME/.cache}/petruterm/shell-context.json"
_petruterm_last_cmd=""

_petruterm_preexec() {
    _petruterm_last_cmd="$1"
}

_petruterm_precmd() {
    local exit_code=$?
    local cache_dir="${_petruterm_context_file%/*}"
    [[ -d "$cache_dir" ]] || mkdir -p "$cache_dir"

    # Minimal JSON escaping: backslash then double-quote.
    local cmd="${_petruterm_last_cmd//\\/\\\\}"
    cmd="${cmd//\"/\\\"}"
    local cwd="${PWD//\\/\\\\}"
    cwd="${cwd//\"/\\\"}"

    printf '{"cwd":"%s","last_command":"%s","last_exit_code":%d}\n' \
        "$cwd" "$cmd" "$exit_code" >| "$_petruterm_context_file"

    _petruterm_last_cmd=""
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec _petruterm_preexec
add-zsh-hook precmd  _petruterm_precmd
