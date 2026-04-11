declare module "@generated/contentFixture.json" {
  const value: unknown;
  export default value;
}

declare module "@generated/resourceIndex.json" {
  const value: unknown;
  export default value;
}

declare module "@generated/uiTheme.json" {
  const value: unknown;
  export default value;
}

declare module "@generated/app_wasm.js" {
  const init: (...args: unknown[]) => Promise<unknown>;
  export default init;
  export const replace_flight_plan_state: (...args: unknown[]) => string;
  export const set_content_policy_state: (...args: unknown[]) => string;
  export const refresh_content_state: (...args: unknown[]) => string;
  export const chart_for_position: (...args: unknown[]) => Promise<string>;
}

declare module "*.svg" {
  const value: string;
  export default value;
}
