import { Volume2, VolumeX } from "lucide-react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { AudioAppSession, LanguageCode, VolumeState } from "../types";
import { hexToRgba } from "../utils/color";
import { VolumeSlider } from "./VolumeSlider";

interface VolumeExpandedProps {
  volume: VolumeState;
  sessions: AudioAppSession[];
  lang: LanguageCode;
  accentColor?: string;
  onMasterVolumeChange: (level: number) => void;
  onMasterMuteToggle: () => void;
  onAppVolumeChange: (pid: number, level: number) => void;
  onAppMuteToggle: (pid: number) => void;
  onAppDragStateChange: (pid: number | null) => void;
}

export function VolumeExpanded({
  volume,
  sessions,
  lang,
  accentColor = DEFAULT_ACCENT,
  onMasterVolumeChange,
  onMasterMuteToggle,
  onAppVolumeChange,
  onAppMuteToggle,
  onAppDragStateChange,
}: VolumeExpandedProps) {
  const t = getTranslations(lang);
  const displayMaster = volume.muted ? 0 : volume.level;

  return (
    <div className="flex h-[146px] flex-col gap-2 pt-0.5">
      {/* Master Volume Control */}
      <div className="flex items-center gap-2.5 rounded-2xl border border-white/10 bg-white/[0.045] px-2.5 py-2">
        <button
          type="button"
          onClick={onMasterMuteToggle}
          title={volume.muted ? t.volume.unmute : t.volume.mute}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-white/10 text-white transition hover:bg-white/20 active:scale-95"
        >
          {volume.muted || volume.level === 0 ? (
            <VolumeX className="h-3.5 w-3.5 text-rose-400" />
          ) : (
            <Volume2 className="h-3.5 w-3.5" style={{ color: accentColor }} />
          )}
        </button>

        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex items-center justify-between gap-2 text-[10.5px] font-medium">
            <span className="truncate text-white/85">{t.volume.masterVolume}</span>
            <span
              className={`shrink-0 tabular-nums font-semibold ${
                volume.muted ? "text-rose-300" : "text-white"
              }`}
            >
              {volume.muted ? t.hud.muted : `${displayMaster}%`}
            </span>
          </div>
          <VolumeSlider
            value={volume.level}
            muted={volume.muted}
            accentColor={accentColor}
            title={t.volume.wheelHint}
            onChange={onMasterVolumeChange}
          />
        </div>
      </div>

      {/* Scrollable Per-App Mixer */}
      <div className="flex min-h-0 flex-1 flex-col gap-1">
        <div className="flex items-center justify-between px-1 text-[9.5px] font-medium uppercase tracking-wider text-white/45">
          <span className="truncate">{t.volume.appMixer}</span>
          {sessions.length > 0 && (
            <span className="tabular-nums text-white/40">{sessions.length}</span>
          )}
        </div>

        {sessions.length === 0 ? (
          <div className="flex flex-1 items-center justify-center rounded-xl border border-white/8 bg-white/[0.02] px-2 text-[11px] text-white/45">
            {t.volume.noApps}
          </div>
        ) : (
          <div className="flex-1 space-y-1 overflow-y-auto pr-1">
            {sessions.map((session) => {
              const peakPct = Math.round(Math.min(1, Math.max(0, session.peak)) * 100);
              const effVol = session.muted ? 0 : session.volume;
              return (
                <div
                  key={session.pid}
                  className="flex h-[28px] items-center gap-2 rounded-xl border border-white/8 bg-white/[0.03] px-2"
                >
                  <button
                    type="button"
                    onClick={() => onAppMuteToggle(session.pid)}
                    title={session.muted ? t.volume.unmute : t.volume.mute}
                    className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-white/8 text-white/80 transition hover:bg-white/18 hover:text-white active:scale-95"
                  >
                    {session.muted ? (
                      <VolumeX className="h-3 w-3 text-rose-400" />
                    ) : (
                      <Volume2
                        className="h-3 w-3"
                        style={{
                          color: session.active ? accentColor : "rgba(255,255,255,0.7)",
                        }}
                      />
                    )}
                  </button>

                  <div className="w-[96px] min-w-0 shrink-0">
                    <div className="flex items-center gap-1">
                      <span
                        className="truncate text-[10.5px] font-medium text-white/85"
                        title={`${session.name} (${session.process_name})`}
                      >
                        {session.name}
                      </span>
                    </div>
                    <div className="mt-0.5 h-[2px] w-full overflow-hidden rounded-full bg-white/10">
                      <div
                        className="h-full origin-left transition-transform duration-150"
                        style={{
                          transform: `scaleX(${session.muted ? 0 : peakPct / 100})`,
                          backgroundColor: accentColor,
                          boxShadow:
                            peakPct > 5
                              ? `0 0 6px ${hexToRgba(accentColor, 0.65)}`
                              : undefined,
                        }}
                      />
                    </div>
                  </div>

                  <div className="min-w-0 flex-1">
                    <VolumeSlider
                      value={session.volume}
                      muted={session.muted}
                      accentColor={accentColor}
                      heightClass="h-1.5"
                      title={t.volume.wheelHint}
                      onDragStateChange={(d) =>
                        onAppDragStateChange(d ? session.pid : null)
                      }
                      onChange={(lvl) => onAppVolumeChange(session.pid, lvl)}
                    />
                  </div>

                  <span
                    className={`w-7 shrink-0 text-right text-[10px] tabular-nums ${
                      session.muted ? "text-rose-300" : "text-white/65"
                    }`}
                  >
                    {effVol}%
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
