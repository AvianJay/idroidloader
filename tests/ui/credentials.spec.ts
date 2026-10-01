import { expect, test } from "@playwright/test";

test("Android saved login survives reopening, signs in without resending a password, and can be deleted", async ({ page }) => {
  await page.goto("/tests/harness.html?credentials");
  const save = page.getByRole("checkbox", { name: "Save Credentials" });
  await expect(save).toBeVisible();
  await expect(save).not.toBeChecked();
  await page.getByPlaceholder("Apple ID Email...").fill("fixture@example.invalid");
  await page.getByPlaceholder("Apple ID Password...").fill("public-ui-test-fixture");
  await save.check();
  await page.getByRole("button", { name: "Login", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Saved Logins" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign Out" })).toBeVisible();
  const newLogin = await page.evaluate(() => (window as any).testCalls.find((call: any) => call.command === "login_new"));
  expect(newLogin.args.saveCredentials).toBe(true);
  expect(await page.evaluate(() => localStorage.getItem("fixture-settings"))).not.toContain("public-ui-test-fixture");

  await page.reload();
  await expect(page.getByRole("heading", { name: "Saved Logins" })).toBeVisible();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("button", { name: "Sign Out" })).toBeVisible();
  const storedLogin = await page.evaluate(() => (window as any).testCalls.find((call: any) => call.command === "login_stored"));
  expect(storedLogin.args.email).toBe("fixture@example.invalid");
  expect(storedLogin.args).not.toHaveProperty("password");
  await page.getByRole("button", { name: "Sign Out" }).click();
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Saved Logins" })).not.toBeVisible();
  await page.reload();
  await expect(page.getByPlaceholder("Apple ID Email...")).toBeVisible();
  await expect(page.getByText("fixture@example.invalid", { exact: true })).not.toBeVisible();
});

test("Android credentials remain opt-in", async ({ page }) => {
  await page.goto("/tests/harness.html?credentials");
  await page.getByPlaceholder("Apple ID Email...").fill("unsaved@example.invalid");
  await page.getByPlaceholder("Apple ID Password...").fill("public-unsaved-fixture");
  await page.getByRole("button", { name: "Login", exact: true }).click();
  await expect(page.getByRole("button", { name: "Sign Out" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Saved Logins" })).not.toBeVisible();
  await page.reload();
  await expect(page.getByPlaceholder("Apple ID Email...")).toBeVisible();
});
