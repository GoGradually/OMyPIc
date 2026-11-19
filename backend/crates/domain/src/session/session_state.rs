use std::collections::{HashMap, VecDeque};

use super::{
    BatchDecision, LlmConversationState, LlmPromptContext, ModeType, RecommendationRecord,
    SessionId, TurnBatchingPolicy, TurnRecord,
};
use crate::feedback::FeedbackLanguage;
use crate::question::{QuestionGroupAggregate, QuestionItem};
use crate::shared::text_utils::{java_is_blank, java_trim};

const MIN_CONTINUOUS_BATCH_SIZE: i32 = 1;
const MAX_CONTINUOUS_BATCH_SIZE: i32 = 10;

/// 한 세션의 가변 상태. 공유는 application 쪽에서 `Arc<Mutex<SessionState>>`로 한다.
#[derive(Debug)]
pub struct SessionState {
    session_id: SessionId,
    stt_segments: VecDeque<String>,
    llm_recent_turns: VecDeque<TurnRecord>,
    llm_recent_recommendations: VecDeque<RecommendationRecord>,
    /// 입력 순서를 유지하고 중복이 없다 (Java `LinkedHashSet`).
    selected_group_tags: Vec<String>,
    candidate_group_order: Vec<String>,
    group_question_indices: HashMap<String, usize>,

    mode: ModeType,
    continuous_batch_size: i32,
    completed_group_count_since_last_feedback: i32,
    current_group_cursor: usize,
    llm_turn_count_since_rebase: i32,
    llm_bootstrapped: bool,
    feedback_language: FeedbackLanguage,
    llm_conversation_id: String,
    llm_last_response_id: String,
    llm_summary: String,
}

impl SessionState {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            stt_segments: VecDeque::new(),
            llm_recent_turns: VecDeque::new(),
            llm_recent_recommendations: VecDeque::new(),
            selected_group_tags: Vec::new(),
            candidate_group_order: Vec::new(),
            group_question_indices: HashMap::new(),
            mode: ModeType::Immediate,
            continuous_batch_size: 3,
            completed_group_count_since_last_feedback: 0,
            current_group_cursor: 0,
            llm_turn_count_since_rebase: 0,
            llm_bootstrapped: false,
            feedback_language: FeedbackLanguage::of("ko"),
            llm_conversation_id: String::new(),
            llm_last_response_id: String::new(),
            llm_summary: String::new(),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn mode(&self) -> ModeType {
        self.mode
    }

    pub fn continuous_batch_size(&self) -> i32 {
        self.continuous_batch_size
    }

    pub fn completed_group_count_since_last_feedback(&self) -> i32 {
        self.completed_group_count_since_last_feedback
    }

    pub fn selected_group_tags(&self) -> &[String] {
        &self.selected_group_tags
    }

    pub fn candidate_group_order(&self) -> &[String] {
        &self.candidate_group_order
    }

    pub fn group_question_indices(&self) -> &HashMap<String, usize> {
        &self.group_question_indices
    }

    pub fn apply_mode_update(
        &mut self,
        mode: Option<ModeType>,
        continuous_batch_size: Option<i32>,
    ) {
        let previous = self.mode;
        if let Some(mode) = mode {
            self.mode = mode;
        }
        if let Some(size) = continuous_batch_size {
            self.continuous_batch_size =
                size.clamp(MIN_CONTINUOUS_BATCH_SIZE, MAX_CONTINUOUS_BATCH_SIZE);
        }
        if previous != self.mode {
            self.completed_group_count_since_last_feedback = 0;
        }
    }

    pub fn configure_question_groups<S: AsRef<str>, G: AsRef<str>>(
        &mut self,
        selected_tags: impl IntoIterator<Item = S>,
        candidate_group_ids: impl IntoIterator<Item = G>,
    ) {
        self.selected_group_tags.clear();
        for tag in selected_tags {
            let tag = tag.as_ref();
            if !self.selected_group_tags.iter().any(|t| t == tag) {
                self.selected_group_tags.push(tag.to_string());
            }
        }
        self.candidate_group_order = candidate_group_ids
            .into_iter()
            .filter(|id| !java_is_blank(id.as_ref()))
            .map(|id| id.as_ref().to_string())
            .collect();
        self.group_question_indices.clear();
        self.current_group_cursor = 0;
        self.completed_group_count_since_last_feedback = 0;
    }

    pub fn decide_feedback_batch_on_turn(
        &mut self,
        completed_group_this_turn: bool,
    ) -> BatchDecision {
        let decision = TurnBatchingPolicy::on_turn(
            self.mode,
            self.completed_group_count_since_last_feedback,
            self.continuous_batch_size,
            completed_group_this_turn,
        );
        self.completed_group_count_since_last_feedback = decision.next_completed_group_count();
        decision
    }

    pub fn should_generate_feedback(&mut self) -> bool {
        self.decide_feedback_batch_on_turn(true).emit_feedback()
    }

    pub fn should_generate_residual_continuous_feedback(
        &self,
        question_exhausted: bool,
        emitted_feedback_this_turn: bool,
    ) -> bool {
        TurnBatchingPolicy::should_emit_residual_continuous_batch(
            self.mode,
            question_exhausted,
            emitted_feedback_this_turn,
        )
    }

    pub fn resolve_feedback_input_text(&self, fallback_text: &str) -> String {
        if self.mode != ModeType::Continuous || self.stt_segments.is_empty() {
            return fallback_text.to_string();
        }
        self.stt_segments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn stt_segments(&self) -> Vec<String> {
        self.stt_segments.iter().cloned().collect()
    }

    pub fn append_segment(&mut self, text: &str) {
        if java_is_blank(text) {
            return;
        }
        self.stt_segments.push_back(text.to_string());
    }

    pub fn feedback_language(&self) -> &FeedbackLanguage {
        &self.feedback_language
    }

    pub fn set_feedback_language(&mut self, feedback_language: FeedbackLanguage) {
        self.feedback_language = feedback_language;
    }

    pub fn conversation_state(&self) -> LlmConversationState {
        LlmConversationState::new(
            &self.llm_conversation_id,
            &self.llm_last_response_id,
            self.llm_turn_count_since_rebase,
        )
    }

    pub fn update_conversation_state(&mut self, state: &LlmConversationState) {
        self.llm_conversation_id = state.conversation_id().to_string();
        self.llm_last_response_id = state.response_id().to_string();
        self.llm_turn_count_since_rebase = state.turn_count_since_rebase();
    }

    pub fn reset_conversation_state(&mut self) {
        self.reset_conversation_state_keeping(true);
    }

    /// Java `resetConversationState(boolean clearBootstrapFlag)`.
    pub fn reset_conversation_state_keeping(&mut self, clear_bootstrap_flag: bool) {
        self.update_conversation_state(&LlmConversationState::empty());
        if clear_bootstrap_flag {
            self.llm_bootstrapped = false;
        }
    }

    pub fn is_llm_bootstrapped(&self) -> bool {
        self.llm_bootstrapped
    }

    pub fn mark_llm_bootstrapped(&mut self) {
        self.llm_bootstrapped = true;
    }

    pub fn set_llm_summary(&mut self, summary: &str) {
        self.llm_summary = java_trim(summary).to_string();
    }

    pub fn build_prompt_context(&self) -> LlmPromptContext {
        LlmPromptContext::new(
            &self.llm_summary,
            self.llm_recent_turns.iter().cloned().collect(),
            self.llm_recent_recommendations.iter().cloned().collect(),
        )
    }

    pub fn append_llm_turn(
        &mut self,
        question: &str,
        answer: &str,
        feedback_summary: &str,
        max_recent_turns: usize,
    ) {
        self.llm_recent_turns
            .push_back(TurnRecord::new(question, answer, feedback_summary));
        let limit = max_recent_turns.max(1);
        while self.llm_recent_turns.len() > limit {
            self.llm_recent_turns.pop_front();
        }
    }

    pub fn append_llm_recommendation_terms(
        &mut self,
        filler_term: &str,
        adjective_term: &str,
        adverb_term: &str,
        max_recent_turns: usize,
    ) {
        self.llm_recent_recommendations
            .push_back(RecommendationRecord::new(
                filler_term,
                adjective_term,
                adverb_term,
            ));
        let limit = max_recent_turns.max(1);
        while self.llm_recent_recommendations.len() > limit {
            self.llm_recent_recommendations.pop_front();
        }
    }

    pub fn should_rebase_conversation(&self, threshold: i32) -> bool {
        self.llm_turn_count_since_rebase >= threshold.max(1)
    }

    pub fn current_candidate_group_id(&self) -> Option<&str> {
        self.candidate_group_order
            .get(self.current_group_cursor)
            .map(String::as_str)
    }

    pub fn move_to_next_group(&mut self) {
        self.current_group_cursor =
            (self.current_group_cursor + 1).min(self.candidate_group_order.len());
    }

    pub fn current_question_index(&self, group_id: &str) -> usize {
        if java_is_blank(group_id) {
            return 0;
        }
        self.group_question_indices
            .get(group_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn mark_question_asked(&mut self, group_id: &str) {
        if java_is_blank(group_id) {
            return;
        }
        *self
            .group_question_indices
            .entry(group_id.to_string())
            .or_insert(0) += 1;
    }

    pub fn next_question<'a>(
        &mut self,
        group: &'a QuestionGroupAggregate,
    ) -> Option<&'a QuestionItem> {
        let group_id = group.id().value();
        let index = self.current_question_index(group_id);
        let item = group.questions().get(index)?;
        self.mark_question_asked(group_id);
        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeSet;

    fn state(id: &str) -> SessionState {
        SessionState::new(SessionId::of(id).unwrap())
    }

    #[test]
    fn decide_feedback_batch_on_turn_in_continuous_mode_uses_completed_group_count() {
        let mut state = state("session-1");
        state.apply_mode_update(Some(ModeType::Continuous), Some(2));

        let waiting_for_completion = state.decide_feedback_batch_on_turn(false);
        assert!(!waiting_for_completion.emit_feedback());
        assert_eq!(0, state.completed_group_count_since_last_feedback());

        let waiting_for_batch = state.decide_feedback_batch_on_turn(true);
        assert!(!waiting_for_batch.emit_feedback());
        assert_eq!(1, state.completed_group_count_since_last_feedback());

        let ready = state.decide_feedback_batch_on_turn(true);
        assert!(ready.emit_feedback());
        assert_eq!(0, state.completed_group_count_since_last_feedback());
    }

    #[test]
    fn configure_question_groups_resets_progress_and_stores_tags() {
        let mut state = state("session-2");
        state.configure_question_groups(["travel", "habit"], ["g1", "g2"]);

        let tags: BTreeSet<&str> = state
            .selected_group_tags()
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(BTreeSet::from(["travel", "habit"]), tags);
        assert_eq!(["g1", "g2"], state.candidate_group_order());
        assert_eq!(Some("g1"), state.current_candidate_group_id());

        state.mark_question_asked("g1");
        assert_eq!(1, state.current_question_index("g1"));

        state.move_to_next_group();
        assert_eq!(Some("g2"), state.current_candidate_group_id());
    }

    #[test]
    fn should_generate_residual_continuous_feedback_only_for_continuous_exhausted_without_emit() {
        let mut state = state("session-3");
        state.apply_mode_update(Some(ModeType::Continuous), Some(3));

        assert!(state.should_generate_residual_continuous_feedback(true, false));
        assert!(!state.should_generate_residual_continuous_feedback(true, true));
        assert!(!state.should_generate_residual_continuous_feedback(false, false));
    }

    #[test]
    fn conversation_state_updates_and_resets() {
        let mut state = state("session-4");
        state.update_conversation_state(&LlmConversationState::new("conv-1", "resp-1", 2));
        state.mark_llm_bootstrapped();

        assert_eq!("conv-1", state.conversation_state().conversation_id());
        assert_eq!("resp-1", state.conversation_state().response_id());
        assert!(state.should_rebase_conversation(2));
        assert!(state.is_llm_bootstrapped());

        state.reset_conversation_state();
        assert_eq!("", state.conversation_state().conversation_id());
        assert_eq!(0, state.conversation_state().turn_count_since_rebase());
        assert!(!state.is_llm_bootstrapped());
    }

    #[test]
    fn reset_conversation_state_without_clearing_bootstrap_keeps_flag() {
        let mut state = state("session-4b");
        state.mark_llm_bootstrapped();
        state.update_conversation_state(&LlmConversationState::new("conv-1", "resp-1", 2));

        state.reset_conversation_state_keeping(false);

        assert!(state.is_llm_bootstrapped());
        assert_eq!("", state.conversation_state().conversation_id());
    }

    #[test]
    fn append_llm_turn_keeps_recent_window() {
        let mut state = state("session-5");
        state.append_llm_turn("q1", "a1", "s1", 2);
        state.append_llm_turn("q2", "a2", "s2", 2);
        state.append_llm_turn("q3", "a3", "s3", 2);

        let context = state.build_prompt_context();
        assert_eq!(2, context.recent_turns().len());
        assert_eq!("q2", context.recent_turns()[0].question());
        assert_eq!("q3", context.recent_turns()[1].question());
    }

    #[test]
    fn append_llm_recommendation_terms_keeps_recent_window() {
        let mut state = state("session-6");
        state.append_llm_recommendation_terms("well", "vivid", "definitely", 2);
        state.append_llm_recommendation_terms("actually", "memorable", "clearly", 2);
        state.append_llm_recommendation_terms("to be honest", "cozy", "personally", 2);

        let context = state.build_prompt_context();
        assert_eq!(2, context.recent_recommendations().len());
        assert_eq!(
            "actually",
            context.recent_recommendations()[0].filler_term()
        );
        assert_eq!(
            "to be honest",
            context.recent_recommendations()[1].filler_term()
        );
    }

    // 아래는 Java 테스트에 없는 동작을 고정한다.

    #[test]
    fn apply_mode_update_clamps_batch_size_and_resets_count_when_mode_changes() {
        let mut state = state("session-7");
        state.apply_mode_update(Some(ModeType::Continuous), Some(99));
        assert_eq!(10, state.continuous_batch_size());
        state.apply_mode_update(None, Some(0));
        assert_eq!(1, state.continuous_batch_size());

        state.apply_mode_update(Some(ModeType::Continuous), Some(3));
        state.decide_feedback_batch_on_turn(true);
        assert_eq!(1, state.completed_group_count_since_last_feedback());
        state.apply_mode_update(Some(ModeType::Immediate), None);
        assert_eq!(0, state.completed_group_count_since_last_feedback());
    }

    #[test]
    fn resolve_feedback_input_text_joins_segments_only_in_continuous_mode() {
        let mut state = state("session-8");
        state.append_segment("first");
        state.append_segment("   ");
        state.append_segment("second");

        assert_eq!("fallback", state.resolve_feedback_input_text("fallback"));
        state.apply_mode_update(Some(ModeType::Continuous), None);
        assert_eq!(
            "first\nsecond",
            state.resolve_feedback_input_text("fallback")
        );
    }

    #[test]
    fn next_question_walks_through_group_questions() {
        let mut state = state("session-9");
        let mut group = QuestionGroupAggregate::create("g", ["t"], Utc::now());
        group.add_question("q1", None, Utc::now()).unwrap();
        group.add_question("q2", None, Utc::now()).unwrap();

        assert_eq!("q1", state.next_question(&group).unwrap().text());
        assert_eq!("q2", state.next_question(&group).unwrap().text());
        assert!(state.next_question(&group).is_none());
        assert_eq!(2, state.current_question_index(group.id().value()));
    }
}
