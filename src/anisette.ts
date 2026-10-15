import { useStore } from "./StoreContext";

export const anisetteServers = [
  ["ani.jkcoxson.com", "Jkcoxson"],
  ["ani2.stikstore.app", "StikStore (secondary)"],
];

// A new preference key retires servers that use the old Android anisette backend.
export const useAnisetteServer = () =>
  useStore<string>("anisetteServerWin", anisetteServers[0][0]);
