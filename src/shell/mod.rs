/// Generate the zsh shell hook script.
pub fn init_zsh() -> String {
    r#"# lmc shell integration — zsh
# Add to .zshrc: eval "$(lmc init zsh)"

if [[ -z "${_LMC_HOOKED}" ]]; then
    export _LMC_HOOKED=1
    export _LMC_SESSION_ID="${$}_$(date +%s)"

    _lmc_preexec() {
        _LMC_CMD="$1"
    }

    _lmc_precmd() {
        local _lmc_exit=$?
        [[ -z "${_LMC_CMD}" ]] && return

        lmc record \
            --cmd "${_LMC_CMD}" \
            --dir "${PWD}" \
            --exit-code ${_lmc_exit} \
            --session-id "${_LMC_SESSION_ID}" \
            --shell zsh &!

        unset _LMC_CMD
    }

    autoload -Uz add-zsh-hook
    add-zsh-hook preexec _lmc_preexec
    add-zsh-hook precmd _lmc_precmd
fi
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_zsh_contains_preexec() {
        let script = init_zsh();
        assert!(script.contains("preexec"), "missing preexec hook");
    }

    #[test]
    fn test_init_zsh_contains_precmd() {
        let script = init_zsh();
        assert!(script.contains("precmd"), "missing precmd hook");
    }

    #[test]
    fn test_init_zsh_contains_session_id() {
        let script = init_zsh();
        assert!(script.contains("_LMC_SESSION_ID"), "missing session ID generation");
    }

    #[test]
    fn test_init_zsh_contains_record_call() {
        let script = init_zsh();
        assert!(script.contains("lmc record"), "missing lmc record invocation");
    }

    #[test]
    fn test_init_zsh_contains_idempotency_guard() {
        let script = init_zsh();
        assert!(script.contains("_LMC_HOOKED"), "missing idempotency guard");
    }

    #[test]
    fn test_init_zsh_contains_background_execution() {
        let script = init_zsh();
        assert!(script.contains("&!"), "missing background disown operator");
    }
}
