# PetruTerm Shell Integration
# version: 2
# Tracks CWD, last command, and exit codes so Petrubot has context.
# Also emits OSC 133 (FTCS) semantic prompt markers for block detection.
#
# Usage — add to ~/.zshrc:
#   source ~/.config/petruterm/shell-integration.zsh

_petruterm_cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/petruterm"
_petruterm_last_cmd=""
_petruterm_first_prompt=1

_petruterm_preexec() {
    _petruterm_last_cmd="$1"
    # B;<cmd> = command start with embedded command text
    # C = output start (immediately before command runs)
    printf '\e]133;B;%s\a' "$1"
    printf '\e]133;C\a'
}

_petruterm_precmd() {
    local exit_code=$?

    # D = command end (skip on very first prompt — no command ran yet)
    if [[ $_petruterm_first_prompt -eq 0 ]]; then
        printf '\e]133;D;%d\a' "$exit_code"
    fi
    _petruterm_first_prompt=0

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

    # A = prompt start (emitted last so cursor row is at the actual prompt line)
    printf '\e]133;A\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec _petruterm_preexec
add-zsh-hook precmd  _petruterm_precmd
