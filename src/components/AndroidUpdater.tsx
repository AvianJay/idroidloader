import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useStore } from "../StoreContext";
import { Dropdown } from "./Dropdown";

type UpdateChannel = "release" | "nightly";
type UpdateInfo = { version: string; versionCode: number; channel: UpdateChannel };
type UpdateResult = {
  status: "unavailable" | "up_to_date" | "available";
  version?: string;
  versionCode?: number;
};

export function AndroidUpdater() {
  const { t } = useTranslation();
  const [savedChannel, setChannel] = useStore<string>("updateChannel", "");
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [result, setResult] = useState<UpdateResult | null>(null);
  const [phase, setPhase] = useState<"idle" | "checking" | "downloading">("idle");
  const [message, setMessage] = useState("");
  const [initError, setInitError] = useState(false);
  const busy = useRef(false);
  const generation = useRef(0);
  const channel: UpdateChannel = savedChannel === "nightly" || savedChannel === "release"
    ? savedChannel : info?.channel ?? "release";

  useEffect(() => {
    let active = true;
    invoke<UpdateInfo>("android_update_info").then(value => {
      if (active) setInfo(value);
    }).catch(() => { if (active) setInitError(true); });
    return () => { active = false; };
  }, []);

  async function check(manual: boolean, selectedChannel = channel) {
    if (busy.current) return;
    busy.current = true;
    const request = ++generation.current;
    setPhase("checking");
    setResult(null);
    setMessage("");
    try {
      const update = await invoke<UpdateResult>("check_android_update", { channel: selectedChannel });
      if (request !== generation.current) return;
      setResult(update);
      if (!manual && update.status === "available") {
        toast.info(t("update.available", { version: update.version }));
      }
    } catch (error) {
      if (request === generation.current) setMessage(updateError(error));
    } finally {
      busy.current = false;
      if (request === generation.current) setPhase("idle");
    }
  }

  // StoreProvider has loaded preferences before this component mounts.
  useEffect(() => {
    if (info) void check(false, channel);
  }, [info, channel]);

  function updateError(error: unknown): string {
    const reason = String(error);
    const code = ["invalid_metadata", "rate_limited", "download_changed", "wrong_package",
      "signature_mismatch", "stale_update", "http_error", "installer_unavailable"]
      .find(value => reason.includes(value)) ?? "network_error";
    return t(`update.errors.${code}`);
  }

  async function install() {
    if (busy.current || !result?.versionCode) return;
    busy.current = true;
    setPhase("downloading");
    setMessage("");
    try {
      const response = await invoke<{ status: "permission_required" | "installer_opened" }>(
        "install_android_update", { channel, versionCode: result.versionCode },
      );
      setMessage(t(`update.${response.status}`));
    } catch (error) {
      setMessage(updateError(error));
    } finally {
      busy.current = false;
      setPhase("idle");
    }
  }

  return (
    <fieldset className="update-settings" disabled={!info || phase !== "idle"}>
      <legend>{t("update.title")}</legend>
      {info && <p className="settings-hint">{t("update.installed", { version: info.version })}</p>}
      <Dropdown
        label={t("update.channel")}
        labelId="update-channel"
        value={channel}
        options={[
          { value: "release", label: t("update.release") },
          { value: "nightly", label: t("update.nightly") },
        ]}
        onChange={value => {
          setResult(null);
          setMessage("");
          setChannel(value);
        }}
      />
      <p className="settings-hint">{t(`update.${channel}_hint`)}</p>
      <div className="settings-buttons">
        <button type="button" onClick={() => void check(true)}>
          {t(phase === "checking" ? "update.checking" : "update.check")}
        </button>
        {result?.status === "available" && <button type="button" className="action-button primary" onClick={() => void install()}>
          {t(phase === "downloading" ? "update.downloading" : "update.install")}
        </button>}
      </div>
      <p className="settings-hint" role="status" aria-live="polite">
        {initError ? t("update.errors.unavailable") : message || (phase === "downloading"
          ? t("update.downloading") : result ? t(`update.${result.status}`, { version: result.version }) : "")}
      </p>
    </fieldset>
  );
}
