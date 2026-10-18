# UI tests

UI tests of the three todo pages, Dioxus, Leptos and Topcoat, driven by
[Playwright](https://playwright.dev) against the [compose stack](../README.md#run-everything-with-docker-compose).
One spec covers all three: they use the same labels.

```sh
docker compose up -d --wait   # from the repository root
cd tests
npm install
npx playwright install chromium
npm test                      # every app
npm run test:dioxus           # or one of them: test:leptos, test:topcoat
npm run report                # the HTML report of the last run
```

| Variable     | Client | Default                     |
|--------------|--------|-----------------------------|
| `DIOXUS_URL` | Dioxus | `http://localhost:8080`     |
| `LEPTOS_URL` | Leptos | `http://localhost:8083`     |
| `TOPCOAT_URL` | Topcoat | `http://localhost:8081`   |

The defaults are the ports compose publishes the apps on. Under OrbStack
the domains work too:
`DIOXUS_URL=https://todo.local LEPTOS_URL=https://leptos.todo.local npm test`.

Every test adds todos with titles of its own and deletes them again, even
when it fails, so the tests run on a list that already has todos in it. The
apps show the same list, so they run one after the other.

## Writing a test

The tests in `ui/` were recorded with
[playwright-cli](https://github.com/microsoft/playwright-cli): open the site,
act on it, and copy the Playwright code each command prints.

```sh
playwright-cli open http://localhost:8080
playwright-cli snapshot
playwright-cli fill e5 "Buy milk" --submit
playwright-cli close
```

Find elements by role and label (`getByRole('button', { name: 'Add' })`),
which both clients share, never by CSS class: the two clients are styled
differently.
