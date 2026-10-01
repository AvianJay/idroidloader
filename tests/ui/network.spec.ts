import { expect, test } from "@playwright/test";

test("Android connects using a document URI and does not call desktop USB/updater commands", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/tests/harness.html");
  const connect = page.getByRole("button", { name: "連線至 iPhone", exact: true });
  await expect(connect).toBeDisabled();
  await page.getByLabel("iPhone IP 位址").fill("192.168.1.2");
  await expect(connect).toBeDisabled();
  await page.getByRole("button", { name: "匯入配對檔" }).click();
  await expect(page.getByRole("status").filter({ hasText: "已選擇配對檔" })).toBeVisible();
  await connect.click();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).toBeVisible();
  const calls = await page.evaluate(() => (window as any).testCalls);
  expect(calls.some((call: any) => call.command === "list_devices" || call.command.startsWith("plugin:updater|"))).toBe(false);
  expect(calls.find((call: any) => call.command === "connect_network_device").args).toEqual({ address: "192.168.1.2", pairingPath: "content://test-provider/pairing.plist" });
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await page.evaluate(() => scrollTo(0, 0));
  await page.screenshot({ path: "test-results/android-network.png", fullPage: true });
  await page.getByRole("button", { name: "中斷連線" }).click();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).not.toBeVisible();
  expect(errors).toEqual([]);
});

test("failed connection permits retry and keeps a working selection", async ({ page }) => {
  await page.goto("/tests/harness.html");
  await page.getByLabel("iPhone IP 位址").fill("192.168.1.2");
  await page.getByRole("button", { name: "匯入配對檔" }).click();
  await page.getByRole("button", { name: "連線至 iPhone", exact: true }).click();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).toBeVisible();
  await page.getByLabel("iPhone IP 位址").fill("192.168.1.99");
  await page.getByRole("button", { name: "連線至 iPhone", exact: true }).click();
  await expect(page.getByText("Connection timed out", { exact: true })).toBeVisible();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).toBeVisible();
  await expect(page.getByRole("button", { name: "連線至 iPhone", exact: true })).toBeEnabled();
});
