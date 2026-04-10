# PetruTerm Shell Integration
# version: 1
# Tracks CWD, last command, and exit codes so Petrubot has context.
#
# Usage — add to ~/.zshrc:
#   source ~/.config/petruterm/shell-integration.zsh

_petruterm_cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/petruterm"
_petruterm_last_cmd=""

_petruterm_preexec() {
    _petruterm_last_cmd="$1"
}

_petruterm_precmd() {
    local exit_code=$?
    [[ -d "$_petruterm_cache_dir" ]] || mkdir -p "$_petruterm_cache_dir"

    # Minimal JSON escaping: backslash then double-quote.
    local cmd="${_petruterm_last_cmd//\\/\\\\}"
    cmd="${cmd//\"/\\\"}"
    local cwd="${PWD//\\/\\\\}"
    cwd="${cwd//\"/\\\"}"

    # Write per-PID file so each pane tracks its own shell state.
    printf '{"cwd":"%s","last_command":"%s","last_exit_code":%d}\n' \
        "$cwd" "$cmd" "$exit_code" >| "${_petruterm_cache_dir}/shell-context-$$.json"

    _petruterm_last_cmd=""
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec _petruterm_preexec
add-zsh-hook precmd  _petruterm_precmd
