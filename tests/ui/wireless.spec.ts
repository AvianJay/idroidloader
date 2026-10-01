import { expect, test } from "@playwright/test";

test("wireless onboarding shows the PIN and selects iPhone without importing a file", async ({ page }) => {
  await page.goto("/tests/harness.html?wireless");
  await page.getByRole("button", { name: "Pair wirelessly", exact: true }).click();
  await expect(page.getByLabel("iPhone IP address")).toBeDisabled();
  await page.evaluate(() => (window as any).wirelessStatus({ phase: "pin", code: "123456" }));
  await expect(page.getByLabel("Pairing code", { exact: true })).toHaveText("123456");
  await expect(page.getByText("On iPhone, open Settings → Privacy & Security → Developer Mode.")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await page.evaluate(() => (window as any).wirelessStatus({ phase: "connecting" }));
  await expect(page.getByLabel("Pairing code", { exact: true })).toHaveCount(0);
  await page.evaluate(() => (window as any).completeWireless());
  await expect(page.getByText("iOS 27.0 · 192.168.1.27")).toBeVisible();
  await expect(page.getByRole("button", { name: "Pair wirelessly", exact: true })).toBeEnabled();
  const commands = await page.evaluate(() => (window as any).testCalls.map((call: any) => call.command));
  expect(commands).toContain("pair_wireless_device");
  expect(commands).not.toContain("plugin:dialog|open");
  expect(commands).not.toContain("connect_network_device");
});

test("cancellation clears the PIN and ignores a late status from the previous attempt", async ({ page }) => {
  await page.goto("/tests/harness.html?wireless");
  await page.getByRole("button", { name: "Pair wirelessly", exact: true }).click();
  await page.evaluate(() => {
    (window as any).wirelessStatus({ phase: "pin", code: "123456" });
    (window as any).oldWirelessChannel = (window as any).testCalls.find((call: any) => call.command === "pair_wireless_device").args.onStatus;
  });
  await page.getByRole("button", { name: "Cancel wireless pairing", exact: true }).click();
  await expect(page.getByLabel("Pairing code", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Pair wirelessly", exact: true }).click();
  await page.evaluate(() => (window as any).oldWirelessChannel.onmessage({ phase: "pin", code: "654321" }));
  await expect(page.getByLabel("Pairing code", { exact: true })).toHaveCount(0);
  await page.evaluate(() => (window as any).wirelessStatus({ phase: "pin", code: "111222" }));
  await expect(page.getByLabel("Pairing code", { exact: true })).toHaveText("111222");
  await page.getByRole("button", { name: "Cancel wireless pairing", exact: true }).click();
});

test("a failed wireless attempt preserves an existing connection and allows retry", async ({ page }) => {
  await page.goto("/tests/harness.html?wireless");
  await page.getByLabel("iPhone IP address").fill("192.168.1.2");
  await page.getByRole("button", { name: "Import pairing file", exact: true }).click();
  await page.getByRole("button", { name: "Connect to iPhone", exact: true }).click();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).toBeVisible();
  await page.getByRole("button", { name: "Pair wirelessly", exact: true }).click();
  await page.evaluate(() => (window as any).failWireless());
  await expect(page.getByText("Wireless pairing timed out", { exact: true })).toBeVisible();
  await expect(page.getByText("iOS 18.0 · 192.168.1.2")).toBeVisible();
  await expect(page.getByRole("button", { name: "Pair wirelessly", exact: true })).toBeEnabled();
});
