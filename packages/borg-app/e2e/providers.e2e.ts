import { expect, test } from "@playwright/test";

test("provider detail save sends selected default models", async ({ page }) => {
  let latestPutBody: Record<string, unknown> | null = null;

  await page.route("**/health", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ status: "ok" }),
    });
  });

  await page.route("**/api/providers?**", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        providers: [
          {
            provider: "openrouter",
            provider_kind: "openrouter",
            api_key: "sk-or-test",
            base_url: null,
            enabled: true,
            tokens_used: 0,
            last_used: null,
            default_text_model: null,
            default_audio_model: null,
            created_at: "2026-03-01T00:00:00.000Z",
            updated_at: "2026-03-01T00:00:00.000Z",
          },
        ],
      }),
    });
  });

  await page.route("**/api/providers/openrouter/models", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        provider: "openrouter",
        models: [
          "openrouter/kimi-k2",
          "openrouter/claude-3.7-sonnet",
          "openrouter/whisper-1",
        ],
      }),
    });
  });

  await page.route("**/api/providers/openrouter", async (route, request) => {
    if (request.method() === "PUT") {
      latestPutBody = request.postDataJSON() as Record<string, unknown>;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ok: true }),
      });
      return;
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        provider: {
          provider: "openrouter",
          provider_kind: "openrouter",
          api_key: "sk-or-test",
          base_url: null,
          enabled: true,
          tokens_used: 0,
          last_used: null,
          default_text_model: null,
          default_audio_model: null,
          created_at: "2026-03-01T00:00:00.000Z",
          updated_at: "2026-03-01T00:00:00.000Z",
        },
      }),
    });
  });

  await page.goto("/settings/providers");

  await page.getByRole("button", { name: "Edit" }).click();
  await expect(page).toHaveURL(/\/settings\/providers\/openrouter$/);

  const chatInput = page.getByPlaceholder("Search and select chat model");
  await chatInput.click();
  await chatInput.fill("kimi");
  await page.getByText("openrouter/kimi-k2", { exact: true }).click();

  const audioInput = page.getByPlaceholder("Search and select audio model");
  await audioInput.click();
  await audioInput.fill("whisper");
  await page.getByText("openrouter/whisper-1", { exact: true }).click();

  await page.getByRole("button", { name: "Save Provider" }).click();

  await expect.poll(() => latestPutBody).not.toBeNull();
  expect(latestPutBody?.default_text_model).toBe("openrouter/kimi-k2");
  expect(latestPutBody?.default_audio_model).toBe("openrouter/whisper-1");
});
