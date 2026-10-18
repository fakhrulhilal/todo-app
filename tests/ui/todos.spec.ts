import { expect, test, type Page } from '@playwright/test';

// Every app uses the same labels: "New todo", "Add", "Done", "Edit",
// "Delete", "Title" and the "Filter todos" controls. What a control looks like
// differs (a filter is a link in Dioxus and Leptos, a button in Topcoat; "Done"
// is a pressed button or a checkbox), so whether a todo is completed is read
// from the filtered lists instead.

/** Project metadata, set in playwright.config.ts. */
type App = { api: boolean; filterInUrl: boolean };

function app(): App {
  return test.info().project.metadata as App;
}

/** The titles this test made, deleted again after it even when it fails. */
let made: string[] = [];

/** A title no other test or earlier run has used. */
function unique(title: string): string {
  const made_ = `${title} ${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  made.push(made_);
  return made_;
}

/** The row of the todo with this title. */
function row(page: Page, title: string) {
  return page.getByRole('listitem').filter({ hasText: title });
}

async function add(page: Page, title: string) {
  const input = page.getByRole('textbox', { name: 'New todo' });
  await input.fill(title);
  await page.getByRole('button', { name: 'Add' }).click();
  await expect(row(page, title)).toBeVisible();
  // The form is cleared for the next one.
  await expect(input).toHaveValue('');
}

/** A filter control: a link or a button, depending on the app. */
function filter(page: Page, name: 'All' | 'Active' | 'Completed') {
  const filters = page.getByRole('navigation', { name: 'Filter todos' });
  return filters.getByRole('link', { name, exact: true }).or(filters.getByRole('button', { name, exact: true }));
}

async function showFilter(page: Page, name: 'All' | 'Active' | 'Completed') {
  const control = filter(page, name);
  await control.click();
  await expect(control).toHaveAttribute('aria-current', 'page');
}

test.afterEach(async ({ page }) => {
  const titles = made;
  made = [];
  if (titles.length === 0) return;
  await page.goto('/');
  await expect(page.getByText(/^\d+ todos?$|^Nothing to show here\.$/)).toBeVisible();
  for (const title of titles) {
    const left = row(page, title);
    if ((await left.count()) > 0) {
      await left.getByRole('button', { name: 'Delete' }).click();
      await expect(left).toHaveCount(0);
    }
  }
});

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // The list has loaded once the count or the empty notice shows.
  await expect(page.getByText(/^\d+ todos?$|^Nothing to show here\.$/)).toBeVisible();
});

test('the page has the form and the filters', async ({ page }) => {
  await expect(page).toHaveTitle('Todos');
  await expect(page.getByRole('textbox', { name: 'New todo' })).toBeFocused();
  for (const name of ['All', 'Active', 'Completed'] as const) {
    await expect(filter(page, name)).toBeVisible();
  }
  await expect(filter(page, 'All')).toHaveAttribute('aria-current', 'page');

  // Only an app with a JSON API links to its reference.
  const docs = page.getByRole('link', { name: 'API docs' });
  if (app().api) {
    await expect(docs).toHaveAttribute('href', '/docs/');
  } else {
    await expect(docs).toHaveCount(0);
  }
});

test('a todo is added, completed, renamed and deleted', async ({ page }) => {
  const title = unique('Buy milk');
  await add(page, title);

  // Complete it: it leaves Active and shows under Completed.
  await row(page, title).getByLabel('Done').click();
  await showFilter(page, 'Active');
  await expect(row(page, title)).toHaveCount(0);
  await showFilter(page, 'Completed');
  await expect(row(page, title)).toBeVisible();

  // And back again.
  await row(page, title).getByLabel('Done').click();
  await expect(row(page, title)).toHaveCount(0);
  await showFilter(page, 'Active');
  await expect(row(page, title)).toBeVisible();

  // Rename it with Enter.
  const renamed = unique('Buy oat milk');
  await row(page, title).getByRole('button', { name: 'Edit' }).click();
  const edit = page.getByRole('textbox', { name: 'Title' });
  await expect(edit).toBeFocused();
  await edit.fill(renamed);
  await edit.press('Enter');
  await expect(row(page, renamed)).toBeVisible();
  await expect(row(page, title)).toHaveCount(0);

  // It was saved, not only changed on screen.
  await page.reload();
  await expect(row(page, renamed)).toBeVisible();

  await row(page, renamed).getByRole('button', { name: 'Delete' }).click();
  await expect(row(page, renamed)).toHaveCount(0);
  await page.reload();
  await showFilter(page, 'All');
  await expect(row(page, renamed)).toHaveCount(0);
});

test('Escape leaves a todo as it was', async ({ page }) => {
  const title = unique('Walk the dog');
  await add(page, title);

  await row(page, title).getByRole('button', { name: 'Edit' }).click();
  const edit = page.getByRole('textbox', { name: 'Title' });
  await edit.fill('Something else');
  await edit.press('Escape');
  await expect(edit).toHaveCount(0);
  await expect(row(page, title)).toBeVisible();

  await row(page, title).getByRole('button', { name: 'Delete' }).click();
  await expect(row(page, title)).toHaveCount(0);
});

test('a blank title is refused with a message', async ({ page }) => {
  // Spaces pass the form's `required`, so the app refuses the title itself.
  // Each app words the message a little differently around the same reason.
  await page.getByRole('textbox', { name: 'New todo' }).fill('   ');
  await page.getByRole('button', { name: 'Add' }).click();
  await expect(page.getByRole('alert')).toContainText(/title must not be empty/i);

  // The next successful call clears the message.
  const title = unique('Water the plants');
  await add(page, title);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await row(page, title).getByRole('button', { name: 'Delete' }).click();
});

test('the address keeps the filter', async ({ page }) => {
  test.skip(!app().filterInUrl, 'the filter is page state here, not part of the address');
  await showFilter(page, 'Completed');
  await expect(page).toHaveURL(/filter=completed/);
  await page.reload();
  await expect(filter(page, 'Completed')).toHaveAttribute('aria-current', 'page');
});
