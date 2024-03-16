use router_bridge::planner::PlanSuccess;

use crate::query_planner::QueryPlanResult;

#[allow(unreachable_code, unused)] // TODO remove once implemented
pub(crate) fn convert_query_plan_from_fed_next(
    plan: apollo_federation::query_plan::QueryPlan,
) -> PlanSuccess<QueryPlanResult> {
    let data = QueryPlanResult {
        formatted_query_plan: todo!(),
        query_plan: todo!(),
    };
    PlanSuccess {
        data,
        usage_reporting: todo!(),
    }
}
