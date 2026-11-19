pub trait RagPolicy: Send + Sync {
    fn get_max_context_chunks(&self) -> usize;
}
