/** True in the end-to-end test build, which accepts `test:run` messages. */
declare const __E2E__: boolean;

declare module "*.css" {
  const text: string;
  export default text;
}
