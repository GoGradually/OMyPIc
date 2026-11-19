use std::collections::VecDeque;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct WrongNoteWindow {
    capacity: usize,
    queue: VecDeque<String>,
}

impl WrongNoteWindow {
    #[allow(clippy::should_implement_trait)]
    pub fn from<S: AsRef<str>>(
        initial_patterns: impl IntoIterator<Item = S>,
        capacity: usize,
    ) -> AppResult<Self> {
        if capacity == 0 {
            return Err(AppError::InvalidArgument(
                "WrongNoteWindow capacity must be positive".to_string(),
            ));
        }
        let mut queue: VecDeque<String> = initial_patterns
            .into_iter()
            .filter(|p| !p.as_ref().trim().is_empty())
            .map(|p| p.as_ref().to_string())
            .collect();
        while queue.len() > capacity {
            queue.pop_front();
        }
        Ok(Self { capacity, queue })
    }

    /// 패턴을 추가하고, 용량을 넘어 밀려난 가장 오래된 패턴을 돌려준다.
    pub fn append(&mut self, pattern: &str) -> Option<String> {
        if pattern.trim().is_empty() {
            return None;
        }
        self.queue.push_back(pattern.to_string());
        if self.queue.len() <= self.capacity {
            return None;
        }
        self.queue.pop_front()
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.queue.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_evicts_oldest_pattern_when_capacity_exceeded() {
        let mut window = WrongNoteWindow::from(["a", "b"], 2).unwrap();

        let evicted = window.append("c");

        assert_eq!(Some("a".to_string()), evicted);
        assert_eq!(vec!["b", "c"], window.snapshot());
    }

    #[test]
    fn append_returns_null_when_within_capacity() {
        let mut window = WrongNoteWindow::from(["a"], 2).unwrap();

        let evicted = window.append("b");

        assert_eq!(None, evicted);
        assert_eq!(vec!["a", "b"], window.snapshot());
    }
}
