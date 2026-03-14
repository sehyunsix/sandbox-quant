use std::fmt::{Display, Formatter};

use crate::charting::scene::{ChartScene, RenderRequest, RenderedFrame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for RenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

pub trait ChartRenderer {
    fn render(
        &self,
        scene: &ChartScene,
        request: &RenderRequest,
    ) -> Result<RenderedFrame, RenderError>;
}
