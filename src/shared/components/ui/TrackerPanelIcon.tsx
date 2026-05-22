import type { ImgHTMLAttributes } from "react";

type TrackerPanelIconProps = Omit<ImgHTMLAttributes<HTMLImageElement>, "alt" | "height" | "src" | "width"> & {
  size?: number | string;
  strokeWidth?: number;
};

const TRACKER_PANEL_ICON_SRC = "/icons/tracker-panel-rpg-icon.svg";

export function TrackerPanelIcon({
  size = "1em",
  strokeWidth: _strokeWidth,
  className,
  style,
  ...props
}: TrackerPanelIconProps) {
  const iconSize = typeof size === "number" ? `${size}px` : size;

  return (
    <img
      src={TRACKER_PANEL_ICON_SRC}
      alt=""
      aria-hidden="true"
      draggable={false}
      className={className}
      style={{ height: iconSize, width: iconSize, ...style }}
      {...props}
    />
  );
}
