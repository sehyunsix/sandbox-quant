use crate::v1::execution::planner::ExecutionPlan;

#[derive(Debug, Default)]
pub struct SpotExecutionPlanner;

impl SpotExecutionPlanner {
    pub fn plan(&self, plan: ExecutionPlan) -> ExecutionPlan {
        plan
    }
}
