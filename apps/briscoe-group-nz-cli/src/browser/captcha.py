"""Mint one reCAPTCHA token on a Briscoe Group storefront, and print it.

Run by `bgnz auth login`, never by hand. Reads its inputs from the environment
-- BGNZ_ORIGIN, BGNZ_HEADLESS -- and prints one JSON document on stdout;
progress goes to stderr.

Note what it is *not* given: no email, and no password. Those never enter the
browser. Gigya's `accounts.login` refuses any request without a reCAPTCHA token
and refuses it before it looks at the password, so a token is the only thing a
browser is needed for -- and the sign-in itself is an ordinary form POST that
the calling program makes. That split is deliberate: it keeps credentials out
of a subprocess, and it lets the caller ask for a session that outlives a
browser tab, which the site's own login never does.

reCAPTCHA v3 is invisible and scored rather than solved, so there is nothing to
click. Three things have to be true, and each was a separate failure before it
was handled:

* **The sign-in screen has to be open.** Gigya only pulls in Google's script
  when it shows the login screen set -- a home page left to settle never loads
  it at all. Hence /login, and the `showScreenSet` fallback for a page that was
  too slow to do it itself.
* **The evaluation has to run in the page's own world.** Camoufox isolates
  injected script by default, where `window.grecaptcha` and `window.gigya` are
  both undefined; `main_world_eval` and the `mw:` prefix opt out of that.
* **The script has to have run, not merely been requested.** Waiting for the
  network to mention recaptcha is too early by several seconds.

Headless is the default and scores well enough in practice; a window
(BGNZ_HEADLESS unset) gives the scorer more to look at and is the one to reach
for when a run is refused.
"""

import json
import os
import re
import sys
import time

from camoufox.sync_api import Camoufox

ORIGIN = os.environ.get("BGNZ_ORIGIN", "https://www.briscoes.co.nz")
# The route that opens the sign-in screen, and so the only one that loads the
# captcha. Both fascias serve it.
LOGIN_PATH = os.environ.get("BGNZ_LOGIN_PATH", "/login")
# Named by the storefront's own config, passed in rather than hardcoded.
SCREEN_SET = os.environ.get("BGNZ_SCREEN_SET", "")
START_SCREEN = os.environ.get("BGNZ_START_SCREEN", "")
HEADLESS = os.environ.get("BGNZ_HEADLESS", "") not in ("", "0", "false", "no")
TIMEOUT = int(os.environ.get("BGNZ_BROWSER_TIMEOUT", "90")) * 1000
# How long to wait for the SDK to bootstrap and Google's script to run.
READY_SECONDS = 45
# reCAPTCHA v3 scores an action name. Gigya configures none, so this is the
# conventional one; Google treats it as advisory.
ACTION = os.environ.get("BGNZ_CAPTCHA_ACTION", "login")

# What the script is doing, for the benefit of a failure that is not one of the
# ones handled below.
STEP = "starting the browser"


def note(message):
    global STEP
    STEP = message
    print(f"bgnz: {message}", file=sys.stderr, flush=True)


def fail(message):
    print(json.dumps({"error": message}))
    sys.exit(1)


def main():
    note(f"opening {ORIGIN}{LOGIN_PATH}")
    with Camoufox(
        headless=HEADLESS,
        humanize=True,
        geoip=True,
        # Without this every evaluation below runs in an isolated world and
        # sees neither `grecaptcha` nor `gigya`.
        main_world_eval=True,
    ) as browser:
        context = browser.new_context()
        page = context.new_page()

        # A fallback source for the site key, in case the SDK's config moves.
        # It arrives as the `render` parameter on the script Gigya loads.
        sniffed = {}

        def watch(request):
            if "recaptcha/api.js" in request.url and "render=" in request.url:
                match = re.search(r"[?&]render=([^&]+)", request.url)
                if match and match.group(1) not in ("explicit", "onload"):
                    sniffed.setdefault("key", match.group(1))

        context.on("request", watch)
        page.goto(ORIGIN + LOGIN_PATH, wait_until="domcontentloaded", timeout=TIMEOUT)

        note("waiting for the sign-in screen to load its captcha")
        deadline = time.time() + READY_SECONDS
        asked = False
        ready = False
        while time.time() < deadline:
            page.wait_for_timeout(500)
            if page.evaluate(
                "mw:!!(window.grecaptcha && typeof window.grecaptcha.execute === 'function')"
            ):
                ready = True
                break
            # Halfway through, open the screen set by hand. The route normally
            # does it, but only once the SDK has finished bootstrapping, and a
            # slow page can miss the storefront's own call.
            if not asked and SCREEN_SET and time.time() > deadline - READY_SECONDS / 2:
                asked = True
                note(f"asking Gigya for screen set {SCREEN_SET!r}")
                page.evaluate(
                    "mw:(() => { try { window.gigya.accounts.showScreenSet("
                    + json.dumps({"screenSet": SCREEN_SET, "startScreen": START_SCREEN})
                    + "); } catch (e) {} })()"
                )

        if not ready:
            fail(
                "the sign-in screen never loaded its captcha, so there is nothing to "
                "mint a token with (the storefront may have changed, or the page did "
                "not finish loading)"
            )

        # The SDK publishes the key it was configured with, which beats the
        # sniffed one: it is what Gigya will verify the token against.
        key = page.evaluate(
            "mw:(() => { try { return window.gigya._.config.captcha.recaptchaV3.siteKey; }"
            " catch (e) { return null; } })()"
        ) or sniffed.get("key")
        if not key:
            fail("the page loaded reCAPTCHA but named no v3 site key")
        note(f"site key {key[:12]}... action {ACTION!r}")

        # The failure is reported rather than swallowed: "no token" and
        # "execute threw" need different advice, and a bare null says neither.
        result = page.evaluate(
            "mw:(async () => { try {"
            "  if (window.grecaptcha.ready) {"
            "    await new Promise((go) => window.grecaptcha.ready(go));"
            "  }"
            "  return { token: await window.grecaptcha.execute("
            + json.dumps(key)
            + ", { action: "
            + json.dumps(ACTION)
            + " }) };"
            "} catch (e) { return { error: String((e && e.message) || e) }; } })()"
        )

        token = (result or {}).get("token")
        if not token:
            detail = (result or {}).get("error")
            fail(
                f"reCAPTCHA refused to mint a token ({detail})"
                if detail
                else "the browser loaded reCAPTCHA but produced no token; "
                "try again with --headful"
            )
        note(f"got a token, {len(token)} characters")

        print(json.dumps({"captcha_token": token, "site_key": key, "action": ACTION}))


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except Exception as crash:
        # A traceback is not an answer. The caller reads one JSON document and
        # otherwise falls back to the last line of stderr, which for a
        # Playwright timeout is a fragment of its own log and says nothing
        # about where it was.
        detail = str(crash).strip().splitlines()
        fail(f"while {STEP}: {detail[0] if detail else type(crash).__name__}")
