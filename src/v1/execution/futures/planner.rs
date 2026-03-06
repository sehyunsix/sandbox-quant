use crate::v1::execution::planner::ExecutionPlan;

#[derive(Debug, Default)]
pub struct FuturesExecutionPlanner;

impl FuturesExecutionPlanner {
    pub fn plan(&self, plan: ExecutionPlan) -> ExecutionPlan {
        plan
    }
}
