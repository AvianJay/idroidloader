import { expect, test } from "@playwright/test";

for (const oldServer of ["ani.sidestore.io", "https://old-custom.example.invalid"]) {
  test(`retires the old Anisette setting ${oldServer} for new logins`, async ({ page }) => {
    await page.addInitScript((server) => {
      localStorage.setItem("fixture-settings", JSON.stringify({ anisetteServer: server }));
    }, oldServer);
    await page.goto("/tests/harness.html?credentials");
    await expect(page.getByRole("button", { name: "Anisette Server:" })).toContainText("Jkcoxson");
    await page.getByPlaceholder("Apple ID Email...").fill("fixture@example.invalid");
    await page.getByPlaceholder("Apple ID Password...").fill("public-login-fixture");
    await page.getByRole("button", { name: "Login", exact: true }).click();
    await expect(page.getByRole("button", { name: "Sign Out" })).toBeVisible();
    const call = await page.evaluate(() => (window as any).testCalls.find((c: any) => c.command === "login_new"));
    expect(call.args.anisetteServer).toBe("ani.jkcoxson.com");
  });
}

test("the supported secondary server persists and is used by saved login", async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem("fixture-settings")) {
      localStorage.setItem("fixture-settings", JSON.stringify({ ids: ["fixture@example.invalid"] }));
    }
  });
  await page.goto("/tests/harness.html?credentials&updater");
  const server = page.getByRole("button", { name: "Anisette Server:" });
  await server.click();
  await expect(page.getByRole("option")).toHaveCount(2);
  await page.getByRole("option", { name: "StikStore (secondary)", exact: true }).click();
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("fixture-settings") || "{}").anisetteServerWin)).toBe("ani2.stikstore.app");
  await page.reload();
  await expect(server).toContainText("StikStore (secondary)");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("button", { name: "Sign Out" })).toBeVisible();
  const call = await page.evaluate(() => (window as any).testCalls.find((c: any) => c.command === "login_stored"));
  expect(call.args.anisetteServer).toBe("ani2.stikstore.app");
});
