import { invoke } from "@tauri-apps/api/core";
import { Check, ClipboardCopy, FileArchive, FolderOpen, Link2, Upload, X } from "lucide-react";
import { useState } from "react";
import { DEFAULT_ACCENT, getTranslations } from "../i18n";
import { LanguageCode } from "../types";
import { hexToRgba } from "../utils/color";

interface DropzoneOverlayProps {
  dragOver: boolean;
  paths: string[];
  lang: LanguageCode;
  accentColor?: string;
  onClear: () => void;
}

function basename(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

export function DropzoneOverlay({
  dragOver,
  paths,
  lang,
  accentColor = DEFAULT_ACCENT,
  onClear,
}: DropzoneOverlayProps) {
  const t = getTranslations(lang);
  const [busy, setBusy] = useState<boolean>(false);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [createdZipPath, setCreatedZipPath] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (dragOver && paths.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center p-2.5">
        <div
          className="flex h-full w-full items-center justify-center gap-2.5 rounded-2xl border border-dashed px-4"
          style={{
            borderColor: hexToRgba(accentColor, 0.7),
            backgroundColor: hexToRgba(accentColor, 0.12),
          }}
        >
          <Upload className="h-4 w-4 animate-bounce" style={{ color: accentColor }} />
          <div className="text-left">
            <p className="text-xs font-semibold text-white">{t.dropzone.dropTitle}</p>
            <p className="text-[10px] text-white/75">{t.dropzone.dropHint}</p>
          </div>
        </div>
      </div>
    );
  }

  const label =
    paths.length === 1
      ? basename(paths[0])
      : t.dropzone.itemsSelected(paths.length, basename(paths[0]));

  const handleCopyFiles = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      const count = await invoke<number>("dropzone_copy_files", { paths });
      setStatusMsg(t.dropzone.copiedFiles(count));
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleCopyPaths = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      await invoke<number>("dropzone_copy_paths", { paths });
      setStatusMsg(t.dropzone.copiedPaths);
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleCreateZip = async () => {
    setBusy(true);
    setErrorMsg(null);
    setCreatedZipPath(null);
    try {
      const zipPath = await invoke<string>("dropzone_create_zip", { paths });
      setCreatedZipPath(zipPath);
      setStatusMsg(t.dropzone.zipReady(basename(zipPath)));
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="flex h-full w-full flex-col justify-between p-3"
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-xs font-semibold text-white">{label}</p>
          <p className="text-[10px] text-white/50">{t.dropzone.title}</p>
        </div>
        <button
          type="button"
          onClick={() => {
            setStatusMsg(null);
            setErrorMsg(null);
            setCreatedZipPath(null);
            onClear();
          }}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-white/10 text-white/70 transition hover:bg-white/20 hover:text-white"
          title={t.dropzone.close}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="grid grid-cols-3 gap-1.5">
        <button
          type="button"
          disabled={busy}
          title={t.dropzone.copyFiles}
          onClick={() => void handleCopyFiles()}
          className="flex min-w-0 items-center justify-center gap-1 rounded-xl border border-white/12 bg-white/8 px-1.5 py-2 text-[10.5px] font-medium text-white transition hover:bg-white/16 disabled:opacity-50"
        >
          <ClipboardCopy className="h-3.5 w-3.5 shrink-0" style={{ color: accentColor }} />
          <span className="truncate">{t.dropzone.copyFiles}</span>
        </button>

        <button
          type="button"
          disabled={busy}
          title={busy ? t.dropzone.processing : t.dropzone.createZip}
          onClick={() => void handleCreateZip()}
          className="flex min-w-0 items-center justify-center gap-1 rounded-xl border px-1.5 py-2 text-[10.5px] font-medium text-white transition disabled:opacity-50"
          style={{
            borderColor: hexToRgba(accentColor, 0.35),
            backgroundColor: hexToRgba(accentColor, 0.22),
          }}
        >
          <FileArchive className="h-3.5 w-3.5 shrink-0" style={{ color: accentColor }} />
          <span className="truncate">{busy ? t.dropzone.processing : t.dropzone.createZip}</span>
        </button>

        <button
          type="button"
          disabled={busy}
          title={t.dropzone.copyPaths}
          onClick={() => void handleCopyPaths()}
          className="flex min-w-0 items-center justify-center gap-1 rounded-xl border border-white/12 bg-white/8 px-1.5 py-2 text-[10.5px] font-medium text-white transition hover:bg-white/16 disabled:opacity-50"
        >
          <Link2 className="h-3.5 w-3.5 shrink-0 text-violet-300" />
          <span className="truncate">{t.dropzone.copyPaths}</span>
        </button>
      </div>

      {(statusMsg || errorMsg) && (
        <div className="flex items-center justify-between gap-2 text-[10.5px]">
          {errorMsg ? (
            <span className="truncate text-rose-300">{errorMsg}</span>
          ) : (
            <span className="flex items-center gap-1 truncate text-emerald-300">
              <Check className="h-3 w-3 shrink-0" />
              {statusMsg}
            </span>
          )}
          {createdZipPath && (
            <button
              type="button"
              onClick={() =>
                void invoke("dropzone_reveal_in_explorer", { path: createdZipPath })
              }
              className="flex shrink-0 items-center gap-1 rounded-lg bg-white/12 px-2 py-0.5 text-[10px] font-medium text-white hover:bg-white/20"
            >
              <FolderOpen className="h-3 w-3" />
              <span>{t.dropzone.reveal}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
