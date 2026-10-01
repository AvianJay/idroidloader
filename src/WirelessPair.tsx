import { useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useError } from "./ErrorContext";
import type { AppError } from "./errors";
import type { DeviceInfo } from "./Device";

type PairingStatus = { phase: "advertising"; name: string }
  | { phase: "pin"; code: string } | { phase: "connecting" };

export function WirelessPair({ disabled, setSelectedDevice, onBusyChange }: {
  disabled: boolean;
  setSelectedDevice: (device: DeviceInfo | null) => void;
  onBusyChange: (busy: boolean) => void;
}) {
  const { t } = useTranslation();
  const { err } = useError();
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<PairingStatus | null>(null);
  const request = useRef(0);
  const active = useRef(false);
  const activeId = useRef<string | null>(null);

  useEffect(() => () => {
    request.current++;
    if (activeId.current) void invoke("cancel_wireless_pairing", { requestId: activeId.current }).catch(() => {});
  }, []);

  const start = async () => {
    if (disabled || active.current) return;
    const id = ++request.current;
    const requestId = crypto.randomUUID();
    activeId.current = requestId;
    active.current = true;
    setBusy(true);
    onBusyChange(true);
    setStatus(null);
    const onStatus = new Channel<PairingStatus>();
    onStatus.onmessage = next => { if (id === request.current) setStatus(next); };
    try {
      const device = await invoke<DeviceInfo>("pair_wireless_device", { requestId, onStatus });
      if (id !== request.current) return;
      setSelectedDevice(device);
      toast.success(t("network.connected", { name: device.name }));
    } catch (error) {
      if (id !== request.current) return;
      const value = error as AppError;
      if (value?.type !== "canceled") toast.error(err(t("wireless.failed"), value));
    } finally {
      if (id === request.current) {
        active.current = false;
        activeId.current = null;
        setBusy(false);
        onBusyChange(false);
        setStatus(null);
      }
    }
  };

  const cancel = async () => {
    const requestId = activeId.current;
    request.current++;
    setStatus(null);
    try { await invoke("cancel_wireless_pairing", { requestId }); }
    finally {
      active.current = false;
      activeId.current = null;
      setBusy(false);
      onBusyChange(false);
    }
  };

  return <section className="wireless-pair" aria-label={t("wireless.title")}>
    <h3>{t("wireless.title")}</h3>
    <p className="network-hint">{t("wireless.requirements")}</p>
    {busy ? <>
      <ol className="wireless-instructions">
        <li>{t("wireless.step_settings")}</li>
        <li>{t("wireless.step_select")}</li>
        <li>{t("wireless.step_pin")}</li>
      </ol>
      <div role="status" aria-live="polite" data-pairing-phase={status?.phase ?? "starting"}>
        {status?.phase === "pin" ? <>
          <p>{t("wireless.pin_hint")}</p>
          <output className="wireless-pin" aria-label={t("wireless.pin_label")}>{status.code}</output>
        </> : <p>{t(status?.phase === "connecting" ? "wireless.connecting" : status ? "wireless.waiting" : "wireless.starting")}</p>}
      </div>
      <button type="button" onClick={() => void cancel()}>{t("wireless.cancel")}</button>
    </> : <button type="button" disabled={disabled} onClick={() => void start()}>{t("wireless.start")}</button>}
  </section>;
}
