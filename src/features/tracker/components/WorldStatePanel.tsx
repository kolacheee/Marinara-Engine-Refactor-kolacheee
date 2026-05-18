import { useEffect, useRef, useState, type ReactNode } from "react";
import { Clock, MapPin } from "lucide-react";
import type { GameState } from "../../../engine/contracts/types/game-state";
import type { GameStatePatchField } from "../../world-state/hooks/use-world-state-patcher";
import { cn } from "../../../shared/lib/utils";
import { WORLD_GRID_BASE_CLASS } from "./tracker-data-sidebar.helpers";
import {
  getForecastWeatherTextClass,
  getLocationPinColor,
  getTemperatureColor,
  getTemperatureGaugeDisplay,
  getWeatherEmoji,
  getWorldAmbienceStyle,
  getWorldDashboardGridClass,
  getWorldDateDisplay,
  getWorldTimeDisplay,
  visibleText,
} from "./tracker-data-sidebar.helpers";
import { SectionHeader } from "./tracker-data-sidebar.controls";

export function WorldStatePanel({
  state,
  action,
  onSaveField,
  collapsed = false,
  onToggleCollapsed,
}: {
  state: GameState | null;
  action?: ReactNode;
  onSaveField: (field: GameStatePatchField, value: string | null) => void;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}) {
  const dashboardGridClass = getWorldDashboardGridClass(state?.weather, state?.temperature, state?.location);

  return (
    <div
      className="relative z-10 overflow-hidden border-b border-[var(--border)] shadow-inner transition-colors duration-200"
      style={getWorldAmbienceStyle(state)}
    >
      <div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-[var(--primary)]/20" />

      <SectionHeader
        icon={<MapPin size="0.6875rem" />}
        title="World"
        action={action}
        collapsed={collapsed}
        onToggle={onToggleCollapsed}
      />

      {!collapsed && (
        <div
          className={cn(
            "relative grid gap-px p-1 @min-[380px]:gap-1 @min-[380px]:p-1.5",
            WORLD_GRID_BASE_CLASS,
            dashboardGridClass,
          )}
        >
          <WorldDateTile value={state?.date} onSave={(value) => onSaveField("date", value || null)} />
          <WorldTimeTile value={state?.time} onSave={(value) => onSaveField("time", value || null)} />
          <WorldForecastTile
            weather={state?.weather}
            temperature={state?.temperature}
            onSaveWeather={(value) => onSaveField("weather", value || null)}
            onSaveTemperature={(value) => onSaveField("temperature", value || null)}
          />
          <WorldLocationPlate
            value={state?.location}
            onSave={(value) => onSaveField("location", value || null)}
            className="col-span-3 @min-[380px]:col-span-1 @min-[380px]:col-start-4 @min-[380px]:row-start-1 @min-[380px]:min-h-[3.125rem]"
          />
        </div>
      )}
    </div>
  );
}

function WorldTileShell({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        "relative min-h-[3.125rem] min-w-0 overflow-hidden rounded-sm border border-[var(--border)]/36 bg-[color-mix(in_srgb,var(--background)_36%,transparent)] shadow-[inset_0_1px_0_color-mix(in_srgb,var(--foreground)_7%,transparent)]",
        className,
      )}
      title={label}
    >
      <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(135deg,color-mix(in_srgb,var(--foreground)_5%,transparent),transparent_46%,color-mix(in_srgb,var(--primary)_8%,transparent))]" />
      <span className="sr-only">{label}</span>
      <div className="relative z-[1] h-full min-w-0">{children}</div>
    </div>
  );
}

function WorldRenderedEdit({
  label,
  value,
  onSave,
  placeholder,
  className,
  inputClassName,
  children,
}: {
  label: string;
  value: string | null | undefined;
  onSave?: (value: string) => void;
  placeholder?: string;
  className?: string;
  inputClassName?: string;
  children: ReactNode;
}) {
  const currentValue = value === null || value === undefined ? "" : String(value);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(currentValue);
  const inputRef = useRef<HTMLInputElement>(null);
  const title = `${label}: ${visibleText(value)}`;

  useEffect(() => {
    if (!editing) setDraft(currentValue);
  }, [currentValue, editing]);

  useEffect(() => {
    if (!editing) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed !== currentValue) onSave?.(trimmed);
    setEditing(false);
  };

  if (!onSave) {
    return (
      <div className={cn("h-full min-w-0", className)} title={title}>
        {children}
      </div>
    );
  }

  if (editing) {
    return (
      <input
        ref={inputRef}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") {
            setDraft(currentValue);
            setEditing(false);
          }
        }}
        onBlur={commit}
        className={cn(
          "h-full w-full min-w-0 rounded-sm border border-[var(--primary)]/35 bg-[var(--background)]/68 px-1 text-[0.6875rem] font-semibold text-[var(--foreground)] outline-none focus:ring-1 focus:ring-[var(--primary)]",
          inputClassName,
        )}
        placeholder={placeholder ?? `Set ${label.toLowerCase()}`}
        aria-label={label}
      />
    );
  }

  return (
    <button
      type="button"
      onClick={() => setEditing(true)}
      title={title}
      aria-label={`${title}. Click to edit.`}
      className={cn(
        "h-full w-full min-w-0 text-left transition-colors hover:bg-[var(--accent)]/20 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-[var(--primary)]",
        className,
      )}
    >
      {children}
    </button>
  );
}

function WorldDateTile({ value, onSave }: { value: string | null | undefined; onSave?: (value: string) => void }) {
  const display = getWorldDateDisplay(value);
  return (
    <WorldTileShell label="Date">
      <WorldRenderedEdit
        label="Date"
        value={display.raw || value}
        onSave={onSave}
        placeholder="Set date"
        className="grid grid-rows-[0.95rem_minmax(0,1fr)] overflow-hidden text-center"
        inputClassName="text-center"
      >
        <div className="bg-[var(--primary)]/24 text-[0.5rem] font-bold leading-[0.95rem] text-[var(--primary)]">
          {display.month}
        </div>
        <div className="flex min-h-0 flex-col items-center justify-center bg-[var(--background)]/22 text-[var(--foreground)]">
          <span className="text-base font-black leading-none">{display.day}</span>
          {display.year && (
            <span className="mt-0.5 text-[0.5rem] font-semibold leading-none text-[var(--muted-foreground)]/70">
              {display.year}
            </span>
          )}
        </div>
      </WorldRenderedEdit>
    </WorldTileShell>
  );
}

function WorldForecastTile({
  weather,
  temperature,
  onSaveWeather,
  onSaveTemperature,
}: {
  weather: string | null | undefined;
  temperature: string | null | undefined;
  onSaveWeather?: (value: string) => void;
  onSaveTemperature?: (value: string) => void;
}) {
  const weatherText = visibleText(weather, "Set weather");
  const temperatureDisplay = getTemperatureGaugeDisplay(temperature);
  return (
    <WorldTileShell label="Forecast" className="min-h-[3.125rem]">
      <div className="@container relative h-full min-w-0 overflow-hidden">
        <div
          aria-hidden="true"
          className="pointer-events-none absolute right-[2.05rem] top-1/2 z-0 -translate-y-1/2 select-none text-[2.75rem] leading-none opacity-[0.13] saturate-125 @min-[7rem]:right-[2.35rem] @min-[7rem]:text-[3.3rem] @min-[10rem]:right-[2.75rem] @min-[10rem]:text-[4.15rem] @min-[14rem]:right-[3.15rem] @min-[14rem]:text-[4.85rem]"
        >
          {getWeatherEmoji(weather)}
        </div>
        <div className="pointer-events-none absolute inset-0 z-[1] bg-[linear-gradient(90deg,color-mix(in_srgb,var(--background)_20%,transparent)_0%,transparent_44%),radial-gradient(circle_at_82%_45%,color-mix(in_srgb,var(--primary)_14%,transparent)_0%,transparent_54%)]" />
        <WorldRenderedEdit
          label="Weather"
          value={weather}
          onSave={onSaveWeather}
          placeholder="Set weather"
          className="relative z-[2] flex h-full min-w-0 flex-col justify-center overflow-hidden px-1.5 py-1 pr-[2.65rem] text-left @min-[7rem]:pr-[2.95rem] @min-[10rem]:px-2 @min-[10rem]:pr-[3.35rem] @min-[14rem]:pr-[3.85rem]"
          inputClassName="pr-[2.65rem] text-left text-[0.75rem] @min-[7rem]:pr-[2.95rem] @min-[10rem]:pr-[3.35rem] @min-[14rem]:pr-[3.85rem]"
        >
          <span
            className={cn(
              "line-clamp-2 block max-w-full min-w-0 text-balance break-words font-black uppercase tracking-normal text-[var(--foreground)]/92 drop-shadow-sm",
              getForecastWeatherTextClass(weatherText),
            )}
          >
            {weatherText}
          </span>
        </WorldRenderedEdit>
        <div className="absolute bottom-0.5 right-0.5 top-0.5 z-[3] w-[2.3rem] @min-[7rem]:w-[2.45rem] @min-[10rem]:bottom-0.5 @min-[10rem]:top-auto @min-[10rem]:h-[2.95rem] @min-[10rem]:w-[2.7rem] @min-[14rem]:w-[3rem]">
          <WorldRenderedEdit
            label="Temp"
            value={temperature}
            onSave={onSaveTemperature}
            placeholder="Set temp"
            className="flex h-full w-full flex-col items-center justify-center gap-0.5 rounded-[3px] bg-transparent px-0 pb-0.5 pt-0.5 text-center shadow-none hover:!bg-transparent @min-[10rem]:justify-end @min-[10rem]:gap-1 @min-[10rem]:pt-0"
            inputClassName="text-center text-[0.625rem]"
          >
            <span className="flex h-7 w-full min-w-0 items-center justify-center overflow-visible @min-[10rem]:h-7 @min-[14rem]:h-8">
              <span className="origin-center scale-[0.74] @min-[7rem]:scale-[0.78] @min-[10rem]:scale-[0.78] @min-[14rem]:scale-[0.88]">
                <WorldThermometerGauge value={temperature} />
              </span>
            </span>
            <span
              className={cn(
                "min-w-0 truncate text-[0.5625rem] font-black leading-none tracking-normal @min-[10rem]:text-[0.625rem] @min-[14rem]:text-[0.6875rem]",
                getTemperatureColor(temperature),
              )}
            >
              {temperatureDisplay.label}
            </span>
          </WorldRenderedEdit>
        </div>
      </div>
    </WorldTileShell>
  );
}

function WorldThermometerGauge({ value }: { value: string | null | undefined }) {
  const display = getTemperatureGaugeDisplay(value);
  const fillStyle = { backgroundColor: display.color };
  return (
    <div className="relative h-8 w-5">
      <div className="absolute bottom-[0.55rem] left-1/2 h-[1.42rem] w-[0.55rem] -translate-x-1/2 overflow-hidden rounded-full border border-[var(--border)]/42 bg-[var(--background)]/48 shadow-[inset_0_0_5px_rgba(0,0,0,0.3)]">
        <div className="absolute inset-x-0 bottom-[0.1875rem] top-[0.1875rem] overflow-hidden rounded-full">
          <span
            className="absolute bottom-0 left-1/2 w-[0.25rem] -translate-x-1/2 rounded-full shadow-[0_0_7px_color-mix(in_srgb,var(--primary)_16%,transparent)] transition-[height] duration-200"
            style={{ ...fillStyle, height: `${display.percent}%` }}
          />
        </div>
        <span className="absolute left-1/2 top-[0.1875rem] h-1 w-px -translate-x-1/2 rounded-full bg-[var(--foreground)]/16" />
      </div>
      <span
        className="absolute bottom-[0.45rem] left-1/2 z-[1] h-[0.42rem] w-[0.25rem] -translate-x-1/2 shadow-[0_0_7px_color-mix(in_srgb,var(--primary)_14%,transparent)]"
        style={fillStyle}
      />
      <div className="absolute bottom-0 left-1/2 h-3.5 w-3.5 -translate-x-1/2 rounded-full border border-[var(--border)]/42 bg-[var(--background)]/52 shadow-[inset_0_-2px_5px_rgba(0,0,0,0.26),0_0_7px_color-mix(in_srgb,var(--primary)_9%,transparent)]">
        <span className="absolute inset-[0.1875rem] rounded-full" style={fillStyle} />
        <span className="absolute left-[0.25rem] top-[0.25rem] h-1.5 w-1 rounded-full bg-[var(--foreground)]/22" />
      </div>
    </div>
  );
}

function WorldClockFace({ hour, minute }: { hour: number | null; minute: number | null }) {
  const hasTime = hour !== null && minute !== null;
  const minuteRotation = hasTime ? minute * 6 : 0;
  const hourRotation = hasTime ? (hour % 12) * 30 + minute * 0.5 : -45;

  return (
    <div className="relative flex h-7 w-7 items-center justify-center rounded-full border border-[var(--border)]/45 bg-[radial-gradient(circle_at_50%_48%,color-mix(in_srgb,var(--background)_58%,transparent)_0%,color-mix(in_srgb,var(--background)_84%,transparent)_68%,color-mix(in_srgb,var(--foreground)_8%,transparent)_100%)] text-sky-300 shadow-[inset_0_-2px_5px_rgba(0,0,0,0.28),0_0_10px_color-mix(in_srgb,var(--primary)_10%,transparent)]">
      {hasTime ? (
        <>
          <span className="absolute h-1 w-1 rounded-full bg-sky-300 shadow-[0_0_5px_color-mix(in_srgb,var(--primary)_42%,transparent)]" />
          <span
            className="absolute left-1/2 top-1/2 h-[0.5625rem] w-[2px] origin-bottom rounded-full bg-sky-300"
            style={{ transform: `translate(-50%, -100%) rotate(${hourRotation}deg)` }}
          />
          <span
            className="absolute left-1/2 top-1/2 h-[0.78rem] w-[1px] origin-bottom rounded-full bg-[var(--primary)]"
            style={{ transform: `translate(-50%, -100%) rotate(${minuteRotation}deg)` }}
          />
        </>
      ) : (
        <Clock size="0.875rem" />
      )}
    </div>
  );
}

function WorldTimeTile({ value, onSave }: { value: string | null | undefined; onSave?: (value: string) => void }) {
  const display = getWorldTimeDisplay(value);
  return (
    <WorldTileShell label="Time">
      <WorldRenderedEdit
        label="Time"
        value={display.raw || value}
        onSave={onSave}
        placeholder="Set time"
        className="grid grid-rows-[minmax(0,1fr)_0.625rem] px-1 pb-0.5 pt-0.5 text-center"
        inputClassName="text-center"
      >
        <div className="flex min-h-0 items-center justify-center overflow-visible">
          <WorldClockFace hour={display.hour} minute={display.minute} />
        </div>
        <div className="flex min-w-0 max-w-full translate-y-px items-baseline justify-center gap-0.5">
          <span className="truncate text-[0.5625rem] font-black leading-[0.625rem] text-[var(--foreground)]">
            {display.main}
          </span>
          {display.suffix && (
            <span className="shrink-0 text-[0.4375rem] font-bold leading-none text-[var(--muted-foreground)]">
              {display.suffix}
            </span>
          )}
        </div>
      </WorldRenderedEdit>
    </WorldTileShell>
  );
}

function WorldLocationPlate({
  value,
  onSave,
  className,
}: {
  value: string | null | undefined;
  onSave?: (value: string) => void;
  className?: string;
}) {
  return (
    <WorldTileShell label="Location" className={cn("min-h-[2.375rem]", className)}>
      <WorldRenderedEdit
        label="Location"
        value={value}
        onSave={onSave}
        placeholder="Set location"
        className="relative z-[1] flex flex-col items-center justify-start px-1 pb-0.5 pt-0.5 text-center @min-[380px]:justify-center @min-[380px]:gap-0.5 @min-[380px]:px-1.5 @min-[380px]:py-1"
        inputClassName="text-center text-[0.75rem]"
      >
        <div className="relative mb-px flex h-2.5 w-full items-center justify-center overflow-hidden rounded-[2px] bg-[var(--background)]/16 @min-[380px]:mb-0 @min-[380px]:h-3">
          <div className="pointer-events-none absolute inset-0 opacity-[0.17] [background-image:radial-gradient(circle,color-mix(in_srgb,var(--foreground)_44%,transparent)_0.75px,transparent_1px)] [background-size:4px_4px]" />
          <MapPin size="0.75rem" className={cn("relative z-[1] shrink-0 drop-shadow-sm", getLocationPinColor(value))} />
        </div>
        <span className="min-w-0 max-w-full truncate text-center text-[0.75rem] font-bold leading-4 text-[var(--foreground)]/92">
          {visibleText(value, "Set location")}
        </span>
      </WorldRenderedEdit>
    </WorldTileShell>
  );
}
