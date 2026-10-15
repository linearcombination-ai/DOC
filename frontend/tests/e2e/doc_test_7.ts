import { expect, test } from '@playwright/test'

test('visibility of optional settings based on resources chosen', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByText('Français (French)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByTestId('column-fr-f10').check({ timeout: 120000 })
  await page.getByTestId('column-fr-tn').check()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.getByRole('main')).toContainText(
    "Use chapter labels, e.g., 'Chapter 1' instead of '1'"
  )
  await expect(page.getByRole('main')).toContainText('Show visual separator')
  await expect(page.getByRole('main')).toContainText('Include TN book intro')
  await expect(page.getByRole('main')).toContainText('Include TN chapter intro')
  await page.getByRole('button', { name: 'Edit' }).nth(1).click()
  await page.getByTestId('column-fr-tn').uncheck()
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('button', { name: '▶ Show Optional Settings' }).click()
  await expect(page.getByRole('main')).toContainText(
    "Use chapter labels, e.g., 'Chapter 1' instead of '1'"
  )
  await expect(page.getByRole('main')).not.toContainText('Include TN book intro')
  await expect(page.getByRole('main')).not.toContainText('Include TN chapter intro')
  await expect(page.getByRole('main')).toContainText('Show visual separator')
})

test('burmese', async ({ page }) => {
  await page.goto('http://localhost:8001/')
  await page.getByPlaceholder('Search Gateway Languages').click()
  await page.getByPlaceholder('Search Gateway Languages').fill('my')
  await page.getByText('ျမန္မာစာ (Burmese)').click()
  await page.getByRole('button', { name: 'Next' }).click()
  await expect(page.getByText('ရှင်မဿဲခရစ်ဝင်။')).toBeVisible({ timeout: 120000 })
  await page.getByTestId('row-my-mat').check()
  await page.getByTestId('row-my-mrk').check()
  await page.getByTestId('row-my-luk').check()
  await page.getByTestId('column-my-tn').uncheck()
  await expect(page.locator('body')).toContainText('Translation Notes')
  await expect(page.locator('body')).toContainText('Translation Questions')
  await expect(page.locator('body')).toContainText('Translation Words')
  await expect(page.locator('body')).toContainText('Unlocked Literal Bible')
  await page.getByRole('button', { name: 'Next' }).click()
  await page.getByRole('radio', { name: 'PDF' }).click()
  await page.getByText('Use PrinceXml to produce the').click()
  await page.getByRole('button', { name: 'Generate File' }).click()
})
