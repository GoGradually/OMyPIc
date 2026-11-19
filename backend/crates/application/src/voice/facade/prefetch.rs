//! 다음 질문의 룰북 검색과 시스템 프롬프트를 답이 오기 전에 미리 만든다. 실패하면 조용히
//! 건너뛰고, 피드백을 만들 때 직접 찾는다.

use std::sync::Arc;

use omypic_domain::question::QuestionGroup;
use omypic_domain::shared::text_utils::java_is_blank;

use super::context::RuntimeContext;
use super::events::error_message;
use super::{VoiceSessionFacade, MAX_RULEBOOK_DOCUMENTS_PER_TURN};
use crate::question::model::NextQuestion;

struct PrefetchTarget {
    question_id: String,
    question_text: String,
    question_group: Option<QuestionGroup>,
    feedback_language: String,
}

impl VoiceSessionFacade {
    pub(super) fn schedule_turn_prefetch(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
        question: &NextQuestion,
    ) {
        if !context.settings.ai_enabled || context.is_inactive() || question.skipped {
            return;
        }
        let Some(question_id) = question
            .question_id
            .as_deref()
            .filter(|id| !java_is_blank(id))
        else {
            return;
        };
        let target = PrefetchTarget {
            question_id: question_id.to_string(),
            question_text: question.text.clone().unwrap_or_default(),
            question_group: QuestionGroup::from_nullable(question.group.as_deref()),
            feedback_language: context.settings.feedback_language.clone(),
        };
        let this = Arc::clone(self);
        let task_context = Arc::clone(context);
        self.deps.async_executor.execute(Box::new(move || {
            this.prefetch_turn_prompt(&task_context, &target)
        }));
    }

    fn prefetch_turn_prompt(&self, context: &RuntimeContext, target: &PrefetchTarget) {
        if context.is_inactive() {
            return;
        }
        let result = self.deps.feedback_use_case.prefetch_turn_prompt(
            &target.question_id,
            &target.question_text,
            target.question_group.as_ref(),
            Some(&target.feedback_language),
            MAX_RULEBOOK_DOCUMENTS_PER_TURN,
        );
        match result {
            Ok(prefetch) => context.store_prefetched_turn_prompt(prefetch),
            Err(error) => tracing::debug!(
                "feedback prefetch skipped sessionId={} questionId={} reason={}",
                context.session_id,
                target.question_id,
                error_message(&error)
            ),
        }
    }
}
