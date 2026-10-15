# PetruTerm zsh integration. PetruTerm starts zsh with ZDOTDIR pointing at this directory so
# this file runs first. It hands ZDOTDIR back to the user, sources their own .zshenv, and
# registers a one-shot hook that puts the agent shim directory (PETRUTERM_SHIM_DIR) back at
# the front of PATH right before the first prompt, after /etc/zprofile (path_helper) and the
# user's rc files have reordered it.

if [[ -n "${PETRUTERM_ORIG_ZDOTDIR+X}" ]]; then
    if [[ -n "$PETRUTERM_ORIG_ZDOTDIR" ]]; then
        export ZDOTDIR="$PETRUTERM_ORIG_ZDOTDIR"
    else
        unset ZDOTDIR
    fi
    unset PETRUTERM_ORIG_ZDOTDIR
fi

[[ -r "${ZDOTDIR:-$HOME}/.zshenv" ]] && source "${ZDOTDIR:-$HOME}/.zshenv"

if [[ -o interactive && -n "$PETRUTERM_SHIM_DIR" ]]; then
    _petruterm_first_prompt() {
        path=("$PETRUTERM_SHIM_DIR" ${path:#$PETRUTERM_SHIM_DIR})
        add-zsh-hook -d precmd _petruterm_first_prompt
        unfunction _petruterm_first_prompt
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _petruterm_first_prompt
fi
