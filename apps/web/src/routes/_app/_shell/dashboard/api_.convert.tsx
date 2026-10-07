import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/_shell/dashboard/api_/convert")({
  beforeLoad: () => {
    throw redirect({ to: "/dashboard/cloud" });
  },
});
