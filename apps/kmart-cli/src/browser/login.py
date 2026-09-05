"""Sign in to Kmart in a real browser, and hand back both credentials.

Run by `kmart auth login`, never by hand. Reads its inputs from the
environment -- KMART_EMAIL, KMART_PASSWORD, KMART_ORIGIN, KMART_HEADLESS --
because command line arguments are visible to every process on the machine.

Prints one JSON document on stdout and nothing else; progress goes to stderr.

Why a browser at all: Kmart's bot check guards the password submit and cannot
be satisfied by an HTTP client, whatever its TLS fingerprint -- admission is
bound to the client that ran Akamai's sensor script, and only a browser runs
it. Why *this* browser: measured against the live site, a session with
humanised input gets through. Headless is the default and passes in the common
case; a window (KMART_HEADLESS unset) gives the sensor more to score and is the
one to reach for when a headless run is refused.

The flow is deliberately shallow. Rather than driving OAuth ourselves, it
signs in and then lets the storefront's own single-page app finish the PKCE
exchange it started -- so the refresh token is read out of the page's storage
rather than minted here, and there is no second implementation of the flow to
keep correct.

It also navigates the way a person would, home page first and then the account
link, because that is the only route that works: asking for a protected path
outright is answered with an interstitial that never resolves.
"""

import json
import os
import sys
import time
from urllib.parse import urlparse

from camoufox.sync_api import Camoufox

EMAIL = os.environ.get("KMART_EMAIL", "")
PASSWORD = os.environ.get("KMART_PASSWORD", "")
ORIGIN = os.environ.get("KMART_ORIGIN", "https://www.kmart.co.nz")
# Headless by default; the caller sets KMART_HEADLESS="" to show a window. The
# window is the stronger path when the bot check is being stubborn.
HEADLESS = os.environ.get("KMART_HEADLESS", "") not in ("", "0", "false", "no")
TIMEOUT = int(os.environ.get("KMART_BROWSER_TIMEOUT", "120")) * 1000
# How long to wait for Auth0 to hand back to the storefront. Generous: it
# covers a passkey prompt being read and declined.
HANDBACK_SECONDS = 90

# The Auth0 SPA SDK files its cache under a key built from the client id.
STORAGE_PREFIX = "@@auth0spajs@@"


# What the script is doing, for the benefit of a failure that is not one of
# the ones handled below.
STEP = "starting the browser"


def note(message):
    global STEP
    STEP = message
    print(f"kmart: {message}", file=sys.stderr, flush=True)


def fail(message):
    print(json.dumps({"error": message}))
    sys.exit(1)


def blocked(page):
    body = page.content()
    return "Access Denied" in body and "edgesuite" in body


def banner(page):
    """Auth0's own explanation of a refusal, if it rendered one."""
    for selector in ("#error-element-password", "#error-element-username",
                     ".ulp-input-error-message"):
        try:
            text = page.inner_text(selector).strip()
        except Exception:
            continue
        if text:
            return text
    return ""


def settle(page, seconds=4):
    """Give Akamai's sensor script time to run and post.

    Submitting the instant the form appears is refused: the sensor has not
    reported yet, and an unreported session is an unproven one.
    """
    try:
        page.wait_for_load_state("networkidle", timeout=TIMEOUT)
    except Exception:
        pass
    time.sleep(seconds)


def press(page, selector, what):
    """Click the first candidate that is actually clickable.

    Retried, and only after checking the element is visible, because this is
    where a headless run bites: the markup arrives before the page has settled
    around it, and clicking something that then moves is a timeout rather than
    a click. Measured -- one run in two failed here, and the same run succeeded
    unchanged on a second attempt.
    """
    for _ in range(3):
        for handle in page.query_selector_all(selector):
            try:
                if not handle.is_visible():
                    continue
                handle.click(timeout=15000)
                return
            except Exception:
                continue
        settle(page, 2)
    fail(f"could not click {what}")


def storage(page):
    """Everything both web storages hold, as one dict.

    Both, because which one the Auth0 SDK writes to is a configuration option
    and is not visible from outside the page. Reading the pair costs nothing
    and removes a guess.
    """
    return page.evaluate(
        """() => {
            const out = {};
            for (const store of [window.localStorage, window.sessionStorage]) {
                try {
                    for (let i = 0; i < store.length; i++) {
                        const k = store.key(i);
                        out[k] = store.getItem(k);
                    }
                } catch (e) { /* a storage this page cannot reach */ }
            }
            return out;
        }"""
    )


def read_refresh_token(entries):
    """The refresh token, out of what the storages held.

    The SDK files its cache under a key built from the client id and the
    audience, and wraps the token response in `body`. Both shapes are
    tolerated because that wrapper is a detail of the SDK's version.
    """
    for key, raw in entries.items():
        if not key.startswith(STORAGE_PREFIX):
            continue
        try:
            value = json.loads(raw)
        except Exception:
            continue
        body = value.get("body", value)
        token = body.get("refresh_token")
        if token:
            email = body.get("decodedToken", {}).get("user", {}).get("email")
            return token, email
    return None, None


def admission_cookies(context):
    """Akamai's cookies, per country, in the shape the CLI stores them."""
    keep = lambda name: name == "_abck" or name == "ak_bmsc" or name.startswith("bm_")
    out = {}
    for cookie in context.cookies():
        domain = cookie["domain"].lstrip(".")
        if not keep(cookie["name"]):
            continue
        if domain.endswith("kmart.co.nz"):
            out.setdefault("NZ", {})[cookie["name"]] = cookie["value"]
        elif domain.endswith("kmart.com.au"):
            out.setdefault("AU", {})[cookie["name"]] = cookie["value"]
    return out


def main():
    if not EMAIL or not PASSWORD:
        fail("no credentials were passed to the browser")

    with Camoufox(headless=HEADLESS, humanize=True) as browser:
        context = browser.new_context()
        page = context.new_page()

        # The home page, and then the account link on it -- rather than the
        # account URL directly.
        #
        # This is not politeness, it is the only route that works. Requesting
        # a protected path is answered with Akamai's interstitial, which never
        # resolves; measured against the live site, `/account/` fetched
        # directly sits at 2,492 bytes of sensor script indefinitely, from the
        # home page or cold. Clicking the link instead lets the app's own
        # router start the redirect client-side, and Auth0 answers at once.
        note("opening the storefront")
        page.goto(f"{ORIGIN}/", wait_until="domcontentloaded", timeout=TIMEOUT)
        settle(page)
        if blocked(page) or "sec-if-cpt-container" in page.content():
            fail("the bot check refused the home page")

        note("following the account link")
        if page.query_selector("a[href*='/account']") is None:
            fail("the home page carried no account link, so its markup has changed")
        press(page, "a[href*='/account']", "the account link")

        try:
            page.wait_for_selector('input[name="username"]', timeout=TIMEOUT)
        except Exception:
            if blocked(page) or "sec-if-cpt-container" in page.content():
                fail("the bot check refused the sign-in page")
            fail(f"no sign-in form appeared (landed on {page.url.split('?')[0]})")
        settle(page)

        note("submitting the email address")
        page.fill('input[name="username"]', EMAIL)
        press(page, 'button[type="submit"], button[name="action"]', "continue")

        try:
            page.wait_for_selector('input[name="password"]', timeout=TIMEOUT)
        except Exception:
            if blocked(page):
                fail("the bot check refused the email address step")
            fail(banner(page) or "the email address was not accepted")
        settle(page)

        note("submitting the password")
        page.fill('input[name="password"]', PASSWORD)
        press(page, 'button[type="submit"], button[name="action"]', "sign in")
        settle(page)

        # Signing in is finished when the browser is back on the storefront:
        # Auth0 keeps you on its own host until it has accepted the password,
        # and hands you back afterwards. Watching for that is what separates
        # "the password was refused" from "the password worked but the token
        # never turned up" -- two failures that need opposite fixes and that
        # polling storage alone reports identically.
        note("waiting to be handed back to the storefront")
        home = urlparse(ORIGIN).netloc
        deadline = time.time() + HANDBACK_SECONDS
        while time.time() < deadline:
            here = urlparse(page.url).netloc
            if here == home:
                break

            # Auth0 may interrupt a correct password with an offer to enrol a
            # passkey. It cannot be pre-declined in the authorize request, and
            # an account that has already dismissed it never sees it again --
            # so this runs for a first sign-in and is skipped ever after.
            if "passkey-enrollment" in page.url:
                note("declining the passkey prompt")
                press(page,
                      'button[value="abort-passkey-enrollment"], '
                      'button[data-action-button-secondary="true"]',
                      "the passkey prompt away")
                settle(page)
                continue

            if blocked(page):
                fail("the bot check refused the password step")
            message = banner(page)
            if message:
                fail(message)
            time.sleep(2)
        else:
            fail(f"the sign-in never returned to {home} "
                 f"(it stopped at {page.url.split('?')[0]})")

        note(f"signed in; back on {page.url.split('?')[0]}")

        # The app finishes the PKCE exchange itself once it is back, and files
        # the tokens. Poll rather than guess how long that takes.
        note("waiting for the app to file its tokens")
        token, who, entries = None, None, {}
        for _ in range(30):
            entries = storage(page)
            token, who = read_refresh_token(entries)
            if token:
                break
            time.sleep(2)
        if not token:
            # Say what *was* there. "No token appeared" gives nobody anything
            # to go on. Key names are not secret; the values are, and are not
            # printed.
            keys = sorted(
                k for k in entries if "auth" in k.lower() or "token" in k.lower()
            )
            # Reaching here means the password was accepted -- the redirect
            # above proved that -- so this is an extraction problem, and says
            # so rather than casting doubt on the credentials.
            fail(
                f"signed in, but the app filed no refresh token "
                f"(on {page.url.split('?')[0]}; "
                f"auth-ish keys present: {keys or 'none'})"
            )
        note("found the refresh token")

        # Visit the other country too, so one sign-in yields admission for
        # both -- the gateway keeps a separate Akamai origin per storefront.
        other = ("https://www.kmart.com.au" if "co.nz" in ORIGIN
                 else "https://www.kmart.co.nz")
        try:
            note("collecting cookies for the other country")
            page.goto(other, wait_until="domcontentloaded", timeout=TIMEOUT)
            settle(page, 3)
        except Exception:
            pass

        print(json.dumps({
            "refresh_token": token,
            "email": who or EMAIL,
            "cookies": admission_cookies(context),
        }))


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except Exception as crash:
        # A traceback is not an answer. The caller reads one JSON document and
        # otherwise falls back to the last line of stderr, which for a
        # Playwright timeout is a fragment of its own log -- `- performing
        # click action` and nothing about where.
        detail = str(crash).strip().splitlines()
        fail(f"while {STEP}: {detail[0] if detail else type(crash).__name__}")
