declare module "@product-resource-index" {
  const value: unknown;
  export default value;
}

declare module "@product-catalog" {
  const value: unknown;
  export default value;
}

declare module "@shared-ui-theme" {
  const value: unknown;
  export default value;
}

declare module "@shared-bootstrap" {
  const value: unknown;
  export default value;
}

declare module "@generated/app_wasm.js" {
  const init: (...args: unknown[]) => Promise<unknown>;
  export default init;
  export const build_flight_plan_ui: (...args: unknown[]) => string;
  export const activate_leg_ui: (...args: unknown[]) => string;
  export const activate_next_leg_ui: (...args: unknown[]) => string;
  export const delete_component_ui: (...args: unknown[]) => string;
  export const suspend_sequencing_ui: (...args: unknown[]) => string;
  export const unsuspend_sequencing_ui: (...args: unknown[]) => string;
  export const sequence_active_leg_ui: (...args: unknown[]) => string;
  export const insert_airway_materialized_ui: (...args: unknown[]) => string;
  export const replace_airway_materialized_ui: (...args: unknown[]) => string;
  export const prepare_airway_presentation: (...args: unknown[]) => string;
  export const sort_airway_suggestions_for_ui: (...args: unknown[]) => string;
  export const replace_flight_plan_state: (...args: unknown[]) => string;
  export const set_content_policy_state: (...args: unknown[]) => string;
  export const refresh_content_state: (...args: unknown[]) => string;
  export const chart_for_position: (...args: unknown[]) => Promise<string>;
}

declare module "*.svg" {
  const value: string;
  export default value;
}
