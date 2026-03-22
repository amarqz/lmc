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

/// Generate the bash shell hook script.
pub fn init_bash() -> String {
    r#"# lmc shell integration — bash
# Add to .bashrc: eval "$(lmc init bash)"

if [ -z "$_LMC_HOOKED" ]; then
    _LMC_HOOKED=1
    _LMC_SESSION_ID="${$}_$(date +%s)"
    _LMC_CMD=""
    _LMC_INSIDE_PROMPT=0

    _lmc_preexec() {
        [ "$_LMC_INSIDE_PROMPT" = 1 ] && return
        _LMC_CMD="$1"
    }
    trap '_lmc_preexec "$BASH_COMMAND"' DEBUG

    _lmc_prompt_cmd() {
        local exit_code=$?
        _LMC_INSIDE_PROMPT=1
        if [ -n "$_LMC_CMD" ]; then
            local full_cmd
            full_cmd=$(history 1 | sed 's/^ *[0-9]* *//')
            lmc record \
                --cmd "$full_cmd" \
                --dir "$PWD" \
                --exit-code $exit_code \
                --session-id "$_LMC_SESSION_ID" \
                --shell bash \
                >/dev/null 2>&1 & disown
            _LMC_CMD=""
        fi
        _LMC_INSIDE_PROMPT=0
    }
    PROMPT_COMMAND="_lmc_prompt_cmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
"#
    .to_string()
}

/// Generate the fish shell hook script.
pub fn init_fish() -> String {
    r#"# lmc shell integration — fish
# Add to ~/.config/fish/config.fish: lmc init fish | source

if not set -q _LMC_HOOKED
    set -g _LMC_HOOKED 1
    set -g _LMC_SESSION_ID (string join "" $fish_pid "_" (date +%s))
    set -g _LMC_CMD ""

    function _lmc_preexec --on-event fish_preexec
        set -g _LMC_CMD $argv
    end

    function _lmc_postexec --on-event fish_postexec
        set -l exit_code $status
        if test -n "$_LMC_CMD"
            command lmc record \
                --cmd "$_LMC_CMD" \
                --dir "$PWD" \
                --exit-code $exit_code \
                --session-id "$_LMC_SESSION_ID" \
                --shell fish \
                >/dev/null 2>&1 & disown
            set -g _LMC_CMD ""
        end
    end
end
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

    #[test]
    fn test_init_bash_contains_debug_trap() {
        let script = init_bash();
        assert!(script.contains("trap"), "missing DEBUG trap");
        assert!(script.contains("DEBUG"), "missing DEBUG trap type");
    }

    #[test]
    fn test_init_bash_contains_prompt_command() {
        let script = init_bash();
        assert!(script.contains("PROMPT_COMMAND"), "missing PROMPT_COMMAND setup");
    }

    #[test]
    fn test_init_bash_contains_session_id() {
        let script = init_bash();
        assert!(script.contains("_LMC_SESSION_ID"), "missing session ID generation");
        assert!(script.contains("${$}"), "missing PID in session ID");
    }

    #[test]
    fn test_init_bash_contains_record_call() {
        let script = init_bash();
        assert!(script.contains("lmc record"), "missing lmc record invocation");
    }

    #[test]
    fn test_init_bash_contains_idempotency_guard() {
        let script = init_bash();
        assert!(script.contains("_LMC_HOOKED"), "missing idempotency guard");
    }

    #[test]
    fn test_init_bash_contains_background_execution() {
        let script = init_bash();
        assert!(script.contains("& disown"), "missing background disown operator");
    }

    #[test]
    fn test_init_bash_contains_prompt_guard() {
        let script = init_bash();
        assert!(script.contains("_LMC_INSIDE_PROMPT"), "missing prompt guard to prevent DEBUG trap during PROMPT_COMMAND");
    }

    #[test]
    fn test_init_bash_uses_history_for_full_command() {
        let script = init_bash();
        assert!(script.contains("history 1"), "missing history 1 for full command line capture");
    }

    #[test]
    fn test_init_fish_contains_preexec_event() {
        let script = init_fish();
        assert!(script.contains("fish_preexec"), "missing fish_preexec event handler");
    }

    #[test]
    fn test_init_fish_contains_postexec_event() {
        let script = init_fish();
        assert!(script.contains("fish_postexec"), "missing fish_postexec event handler");
    }

    #[test]
    fn test_init_fish_contains_session_id() {
        let script = init_fish();
        assert!(script.contains("_LMC_SESSION_ID"), "missing session ID generation");
        assert!(script.contains("fish_pid"), "missing fish_pid in session ID");
    }

    #[test]
    fn test_init_fish_contains_record_call() {
        let script = init_fish();
        assert!(script.contains("lmc record"), "missing lmc record invocation");
    }

    #[test]
    fn test_init_fish_contains_idempotency_guard() {
        let script = init_fish();
        assert!(script.contains("_LMC_HOOKED"), "missing idempotency guard");
    }

    #[test]
    fn test_init_fish_contains_background_execution() {
        let script = init_fish();
        assert!(script.contains("& disown"), "missing background disown operator");
    }
}
