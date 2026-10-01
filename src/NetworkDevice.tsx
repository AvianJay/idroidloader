import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { DeviceInfo, DeviceProps } from "./Device";
import { useError } from "./ErrorContext";
import { usePlatform } from "./PlatformContext";
import type { AppError } from "./errors";

function appError(error: unknown): AppError {
  if (typeof error === "object" && error !== null && "type" in error && "message" in error) return error as AppError;
  return { type: "misc", message: error instanceof Error ? error.message : String(error) };
}

export function NetworkDevice({ selectedDevice, setSelectedDevice, registerRefresh }: DeviceProps) {
  const { t } = useTranslation();
  const { err } = useError();
  const { platform } = usePlatform();
  const [address, setAddress] = useState(selectedDevice?.address ?? "");
  const [pairingPath, setPairingPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  const active = selectedDevice?.address ? selectedDevice : null;

  const connect = useCallback(async () => {
    if (busy || !pairingPath || !address.trim()) return;
    const id = ++request.current;
    setBusy(true);
    try {
      const device = await invoke<DeviceInfo>("connect_network_device", { address: address.trim(), pairingPath });
      if (id !== request.current) return;
      setSelectedDevice(device);
      toast.success(t("network.connected", { name: device.name }));
    } catch (error) {
      if (id === request.current) toast.error(err(t("network.failed"), appError(error)));
    } finally {
      if (id === request.current) setBusy(false);
    }
  }, [address, pairingPath, busy, setSelectedDevice, t, err]);

  useEffect(() => {
    registerRefresh?.(connect);
    return () => registerRefresh?.(undefined);
  }, [connect, registerRefresh]);

  useEffect(() => () => {
    request.current++;
    invoke("cancel_pairing").catch(() => {});
  }, []);

  return <div className="network-device">
    <h2>{t("network.title")}</h2>
    <p className="network-hint">{t("network.hint")}</p>
    <form className="network-form" onSubmit={(event) => { event.preventDefault(); void connect(); }}>
      <label htmlFor="device-ip">{t("network.address")}</label>
      <input id="device-ip" type="text" inputMode="text" autoCapitalize="none" autoCorrect="off" spellCheck={false}
        placeholder="192.168.1.100" value={address} onChange={(event) => setAddress(event.target.value)} disabled={busy} />
      <button type="button" disabled={busy} onClick={async () => {
        try {
          const path = await open({ multiple: false, ...(platform === "android" ? {} : {
            filters: [{ name: "Pairing file", extensions: ["plist", "mobiledevicepairing"] }]
          }) });
          if (typeof path === "string") setPairingPath(path);
        } catch (error) { toast.error(err(t("network.import_failed"), appError(error))); }
      }}>{pairingPath ? t("network.replace_pairing") : t("network.import_pairing")}</button>
      {pairingPath && <span className="pairing-file-ready" role="status">{t("network.pairing_ready")}</span>}
      <button type="submit" disabled={busy || !pairingPath || !address.trim()}>
        {busy ? t("network.connecting") : t("network.connect")}
      </button>
      {busy && <button type="button" onClick={async () => {
        request.current++;
        await invoke("cancel_pairing");
        setBusy(false);
      }}>{t("common.cancel")}</button>}
    </form>
    {active && <div className="device-card active">
      <div className="device-meta">
        <span className="device-name">{active.name}</span>
        <span className="device-connection">iOS {active.version} · {active.address}</span>
      </div>
      <button type="button" disabled={busy} onClick={async () => {
        await invoke("set_selected_device", { device: null });
        setSelectedDevice(null);
      }}>{t("network.disconnect")}</button>
    </div>}
    <p className="network-hint">{t("network.requirements")}</p>
  </div>;
}
