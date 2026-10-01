import React, { createContext, useContext, useEffect, useState } from "react";
import "./DialogContext.css";

export type Platform = "windows" | "mac" | "linux" | "android" | "ios";

export function detectPlatform(): Platform {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Android/i.test(ua)) return "android";
  if (/iPhone|iPad|iPod/i.test(ua)) return "ios";
  if (ua.includes("Mac")) return "mac";
  if (ua.includes("Linux")) return "linux";
  return "windows";
}

export const PlatformContext = createContext<{ platform: Platform }>({ platform: detectPlatform() });

export const PlatformProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [platform, setPlatform] = useState<Platform>(detectPlatform);

  useEffect(() => {
    setPlatform(detectPlatform());
  }, []);

  return (
    <PlatformContext.Provider
      value={{
        platform,
      }}
    >
      {children}
    </PlatformContext.Provider>
  );
};

export const usePlatform = () => {
  return useContext(PlatformContext);
};
