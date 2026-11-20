//! 제공자 서버가 들고 있는 대화(OpenAI Responses)의 bootstrap, rebase와 잘못된 대화 복구.
//! 대화를 이쪽이 들고 있는 제공자(`is_stateless`)는 매번 시스템 프롬프트 전체를 빈 대화로 보낸다.

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::session::{LlmConversationState, LlmPromptContext};
use omypic_domain::shared::text_utils::java_is_blank;

use super::prompts::build_system_prompt;
use super::recommendation::command_model;
use super::{failure_message, feedback_language, FeedbackUseCase, TurnRequest};
use crate::feedback::model::FeedbackCommand;
use crate::feedback::policy::FeedbackModelPolicy;
use crate::feedback::port::{LlmClient, LlmGenerateResult};
use crate::session::port::{lock_session, SharedSessionState};

/// bootstrap을 마친 대화에는 시스템 프롬프트를 다시 보내지 않는다.
const EMPTY_SYSTEM_PROMPT: &str = "";

impl FeedbackUseCase {
    /// 세션의 서버 대화를 시스템 프롬프트(룰북 없이)로 미리 연다. 이미 열었거나 제공자가 stateless면
    /// 아무것도 하지 않는다. 실패는 감싸지 않고 그대로 돌려준다.
    pub fn bootstrap_conversation(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        feedback_language: Option<&str>,
    ) -> AppResult<()> {
        let session_id = command.session_id.as_deref().unwrap_or("");
        if java_is_blank(session_id) {
            return Err(AppError::InvalidArgument(
                "sessionId is required".to_string(),
            ));
        }
        let state = self.session_state(command)?;
        if lock_session(&state).is_llm_bootstrapped() {
            return Ok(());
        }
        let provider = FeedbackModelPolicy::normalize_provider(command.provider.as_deref())?;
        let client = self.resolve_client(provider)?;
        if client.is_stateless() {
            return Ok(());
        }
        let language = self::feedback_language(feedback_language);
        let system_prompt = build_system_prompt(language.value(), &[]);
        let current = lock_session(&state).conversation_state();
        let next = client.bootstrap(api_key, command_model(command), &system_prompt, &current)?;
        let mut session = lock_session(&state);
        session.update_conversation_state(&next);
        session.mark_llm_bootstrapped();
        Ok(())
    }

    /// 서버 대화가 rebase 주기에 이르면 새 대화로 시작한다. bootstrap 표시는 남긴다.
    pub(super) fn rebase_conversation_if_needed(
        &self,
        client: &dyn LlmClient,
        state: &SharedSessionState,
    ) -> AppResult<()> {
        if client.is_stateless() {
            return Ok(());
        }
        let mut session = lock_session(state);
        if session.should_rebase_conversation(self.conversation_rebase_turns) {
            if session.is_llm_bootstrapped() {
                session.reset_conversation_state_keeping(false);
            } else {
                session.reset_conversation_state();
            }
        }
        Ok(())
    }

    /// 서버가 대화를 모른다고 하면 대화를 비우고(bootstrap했던 세션은 다시 bootstrap한 뒤) 한 번 더
    /// 요청한다.
    pub(super) fn generate_with_conversation_recovery(
        &self,
        client: &dyn LlmClient,
        api_key: &str,
        command: &FeedbackCommand,
        turn: &TurnRequest<'_>,
        state: &SharedSessionState,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult> {
        let model = command_model(command);
        if client.is_stateless() {
            return client.generate(
                api_key,
                model,
                turn.system_prompt,
                turn.user_prompt,
                &LlmConversationState::empty(),
                prompt_context,
            );
        }
        let (conversation_state, bootstrapped_before_request) = {
            let session = lock_session(state);
            (session.conversation_state(), session.is_llm_bootstrapped())
        };
        let request_system_prompt = if bootstrapped_before_request {
            EMPTY_SYSTEM_PROMPT
        } else {
            turn.system_prompt
        };
        let first = match client.generate(
            api_key,
            model,
            request_system_prompt,
            turn.user_prompt,
            &conversation_state,
            prompt_context,
        ) {
            Ok(generated) => return Ok(generated),
            Err(error) => error,
        };
        if !conversation_state.has_conversation_id() || !is_invalid_conversation_error(&first) {
            return Err(first);
        }
        let reset_state = {
            let mut session = lock_session(state);
            session.reset_conversation_state();
            session.conversation_state()
        };
        if bootstrapped_before_request {
            let next = client.bootstrap(api_key, model, turn.system_prompt, &reset_state)?;
            let mut session = lock_session(state);
            session.update_conversation_state(&next);
            session.mark_llm_bootstrapped();
        }
        let (retry_state, bootstrapped) = {
            let session = lock_session(state);
            (session.conversation_state(), session.is_llm_bootstrapped())
        };
        client.generate(
            api_key,
            model,
            if bootstrapped {
                EMPTY_SYSTEM_PROMPT
            } else {
                turn.system_prompt
            },
            turn.user_prompt,
            &retry_state,
            prompt_context,
        )
    }
}

fn is_invalid_conversation_error(error: &AppError) -> bool {
    let message = failure_message(error).to_lowercase();
    message.contains("conversation not found")
        || message.contains("invalid conversation")
        || message.contains("unknown conversation")
        || message.contains("conversation id")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_conversation_error_matches_known_messages_case_insensitively() {
        for message in [
            "Conversation NOT found",
            "invalid conversation",
            "Unknown Conversation: x",
            "bad conversation id",
        ] {
            assert!(is_invalid_conversation_error(&AppError::Internal(
                message.to_string()
            )));
        }
        assert!(!is_invalid_conversation_error(&AppError::Internal(
            "rate limited".to_string()
        )));
    }
}
