import { CalendarDays, Clock, CloudSun, MapPin, Thermometer, X } from "lucide-react";
import { TRACKER_SECTION_AGENT_TYPES } from "../../world-state/lib/tracker-state-display";
import { TrackerSectionRefresh, WorldFieldRow } from "./RoleplayHUDPanelPrimitives";

interface CombinedWorldPanelProps {
  location: string;
  date: string;
  time: string;
  weather: string;
  temperature: string;
  onSaveLocation: (value: string) => void;
  onSaveDate: (value: string) => void;
  onSaveTime: (value: string) => void;
  onSaveWeather: (value: string) => void;
  onSaveTemperature: (value: string) => void;
  weatherEmoji: string;
  pinColor: string;
  tempColor: string;
  onClose: () => void;
  onRerunSingleTracker?: (agentType: string) => void;
  isTrackerRetryBusy?: boolean;
}

export function CombinedWorldPanel({
  location,
  date,
  time,
  weather,
  temperature,
  onSaveLocation,
  onSaveDate,
  onSaveTime,
  onSaveWeather,
  onSaveTemperature,
  weatherEmoji,
  pinColor,
  tempColor,
  onClose,
  onRerunSingleTracker,
  isTrackerRetryBusy,
}: CombinedWorldPanelProps) {
  return (
    <>
      <div className="flex items-center justify-between border-b border-[var(--border)] px-3 py-1.5">
        <span className="text-[0.625rem] font-semibold text-[var(--muted-foreground)] uppercase tracking-wider flex items-center gap-1">
          <CloudSun size="0.625rem" /> World State
        </span>
        <span className="flex items-center gap-1">
          <TrackerSectionRefresh
            agentType={TRACKER_SECTION_AGENT_TYPES.world}
            onRerunSingleTracker={onRerunSingleTracker}
            busy={isTrackerRetryBusy}
            title="Re-run world state tracker only"
          />
          <button
            type="button"
            onClick={onClose}
            className="text-[var(--muted-foreground)]/50 hover:text-[var(--foreground)] transition-colors"
            title="Close"
            aria-label="Close"
          >
            <X size="0.75rem" />
          </button>
        </span>
      </div>
      <div className="divide-y divide-[var(--border)]">
        <WorldFieldRow
          icon={<MapPin size="0.8125rem" className={pinColor} />}
          label="Location"
          value={location}
          onSave={onSaveLocation}
          accent="text-emerald-300"
        />
        <WorldFieldRow
          icon={<CalendarDays size="0.8125rem" className="text-violet-400" />}
          label="Date"
          value={date}
          onSave={onSaveDate}
          accent="text-violet-300"
        />
        <WorldFieldRow
          icon={<Clock size="0.8125rem" className="text-amber-400" />}
          label="Time"
          value={time}
          onSave={onSaveTime}
          accent="text-amber-300"
        />
        <WorldFieldRow
          icon={<span className="text-sm leading-none">{weatherEmoji}</span>}
          label="Weather"
          value={weather}
          onSave={onSaveWeather}
          accent="text-sky-300"
        />
        <WorldFieldRow
          icon={<Thermometer size="0.8125rem" className={tempColor} />}
          label="Temperature"
          value={temperature}
          onSave={onSaveTemperature}
          accent="text-rose-300"
        />
      </div>
    </>
  );
}
