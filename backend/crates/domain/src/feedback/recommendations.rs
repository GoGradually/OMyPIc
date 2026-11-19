use super::RecommendationDetail;

/// Java에서 null이던 항목은 `RecommendationDetail::default()`(빈 값)로 대신한다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recommendations {
    filler: RecommendationDetail,
    adjective: RecommendationDetail,
    adverb: RecommendationDetail,
}

impl Recommendations {
    pub fn new(
        filler: RecommendationDetail,
        adjective: RecommendationDetail,
        adverb: RecommendationDetail,
    ) -> Self {
        Self {
            filler,
            adjective,
            adverb,
        }
    }

    pub fn filler(&self) -> &RecommendationDetail {
        &self.filler
    }

    pub fn adjective(&self) -> &RecommendationDetail {
        &self.adjective
    }

    pub fn adverb(&self) -> &RecommendationDetail {
        &self.adverb
    }
}
