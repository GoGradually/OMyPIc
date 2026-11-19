use super::CorrectionDetail;

/// Java에서 null이던 항목은 `CorrectionDetail::default()`(빈 값)로 대신한다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corrections {
    grammar: CorrectionDetail,
    expression: CorrectionDetail,
    logic: CorrectionDetail,
}

impl Corrections {
    pub fn new(
        grammar: CorrectionDetail,
        expression: CorrectionDetail,
        logic: CorrectionDetail,
    ) -> Self {
        Self {
            grammar,
            expression,
            logic,
        }
    }

    pub fn grammar(&self) -> &CorrectionDetail {
        &self.grammar
    }

    pub fn expression(&self) -> &CorrectionDetail {
        &self.expression
    }

    pub fn logic(&self) -> &CorrectionDetail {
        &self.logic
    }
}
