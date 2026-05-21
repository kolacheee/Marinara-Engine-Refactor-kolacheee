import { cn } from "../../../shared/lib/utils";
import { EMPTY_STATE } from "./RoleplayHUDPanelPrimitives";

export type TrackerSectionLayout = "combined" | "panel";

export type TrackerRetryControls = {
  onRerunSingleTracker?: (agentType: string) => void;
  isTrackerRetryBusy?: boolean;
};

export function sectionPadding(layout: TrackerSectionLayout) {
  return layout === "combined" ? "p-2" : "";
}

export function headerClass(layout: TrackerSectionLayout) {
  return layout === "combined"
    ? "flex items-center justify-between px-1 pb-1"
    : "flex items-center justify-between border-b border-[var(--border)] px-3 py-1.5";
}

export function bodyClass(layout: TrackerSectionLayout, spacing: string) {
  return layout === "combined" ? spacing : cn("p-2", spacing);
}

export function emptyClass(layout: TrackerSectionLayout) {
  return layout === "combined" ? EMPTY_STATE : cn(EMPTY_STATE, "py-2");
}
