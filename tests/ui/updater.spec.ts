import { expect, test } from "@playwright/test";

test("channel selection persists and startup checks use it", async ({ page }) => {
  await page.goto("/tests/harness.html?updater");
  await expect(page.getByText("No Android update has been published for this channel yet.")).toBeVisible();
  await page.getByRole("button", { name: "Update channel" }).click();
  await page.getByRole("option", { name: "Nightly", exact: true }).click();
  await expect(page.getByText("You have the newest available build for this channel.")).toBeVisible();
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("fixture-settings") || "{}").updateChannel)).toBe("nightly");
  await page.reload();
  await expect(page.getByRole("button", { name: "Update channel" })).toContainText("Nightly");
  await expect(page.getByText("You have the newest available build for this channel.")).toBeVisible();
  const channels = await page.evaluate(() => (window as any).testCalls.filter((c: any) => c.command === "check_android_update").map((c: any) => c.args.channel));
  expect(channels).toEqual(["nightly"]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await page.getByRole("group", { name: "App updates" }).screenshot({ path: "test-results/android-updater.png" });
});

test("nightly installations default to their build channel", async ({ page }) => {
  await page.goto("/tests/harness.html?updater&buildChannel=nightly");
  await expect(page.getByRole("button", { name: "Update channel" })).toContainText("Nightly");
  await expect(page.getByText("You have the newest available build for this channel.")).toBeVisible();
});

test("checks recover after an error and updates require an explicit install action", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.goto("/tests/harness.html?updater");
  const check = page.getByRole("button", { name: "Check for updates", exact: true });
  await expect(check).toBeEnabled();
  await page.evaluate(() => { (window as any).updateFixture.error = "rate_limited"; });
  await check.click();
  await expect(page.getByText("GitHub’s request limit was reached. Try again later.")).toBeVisible();
  await page.evaluate(() => {
    (window as any).updateFixture.error = null;
    (window as any).updateFixture.release = { status: "available", version: "2.4.0", versionCode: 100000020 };
    (window as any).updateFixture.install = "permission_required";
  });
  await check.click();
  await expect(page.getByText("Update available: 2.4.0", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as any).testCalls.some((c: any) => c.command === "install_android_update"))).toBe(false);
  await page.getByRole("button", { name: "Download and install", exact: true }).click();
  await expect(page.getByText(/Allow iDroidLoader to install apps in Android Settings/)).toBeVisible();
  await page.evaluate(() => { (window as any).updateFixture.install = "installer_opened"; });
  await page.getByRole("button", { name: "Download and install", exact: true }).click();
  await expect(page.getByText(/Finish the update in Android’s installer/)).toBeVisible();
  const installs = await page.evaluate(() => (window as any).testCalls.filter((c: any) => c.command === "install_android_update"));
  expect(installs.map((call: any) => call.args)).toEqual([
    { channel: "release", versionCode: 100000020 }, { channel: "release", versionCode: 100000020 },
  ]);
  expect(errors).toEqual([]);
});

test("invalid downloads report failure and channel changes discard the offered update", async ({ page }) => {
  await page.goto("/tests/harness.html?updater");
  await expect(page.getByRole("button", { name: "Check for updates", exact: true })).toBeEnabled();
  await page.evaluate(() => {
    (window as any).updateFixture.release = { status: "available", version: "2.4.0", versionCode: 100000020 };
    (window as any).updateFixture.installError = "download_changed";
  });
  await page.getByRole("button", { name: "Check for updates", exact: true }).click();
  await page.getByRole("button", { name: "Download and install", exact: true }).click();
  await expect(page.getByText(/downloaded APK did not match/)).toBeVisible();
  await page.getByRole("button", { name: "Update channel" }).click();
  await page.getByRole("option", { name: "Nightly", exact: true }).click();
  await expect(page.getByRole("button", { name: "Download and install", exact: true })).not.toBeVisible();
  await expect(page.getByText("You have the newest available build for this channel.")).toBeVisible();
});
