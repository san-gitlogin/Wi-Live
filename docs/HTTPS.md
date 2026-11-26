# Trusted https for Wi-Live: the plan, and the decision it needs

Why it matters (D-05): the dashboard is served over plain http. The media is already
encrypted (DTLS-SRTP) and the page checks the fingerprint with the device's key, but
someone able to rewrite traffic on the home network could replace the page itself.
https also unlocks two things browsers keep for secure pages: talk back (the
viewer's microphone, `getUserMedia`) and WebCodecs/WebTransport (an even lower-latency
path than WebRTC's jitter buffer).

## Why it is not done yet

A browser trusts a certificate only if a public authority issued it for a public name.
A computer at `192.168.0.103` has neither. The options:

| option | what the person sees | needs |
|---|---|---|
| self-signed certificate | a full-page warning on every device, every time it changes | nothing: **rejected** (puts the problem on the user, owner 2026-11-25) |
| install a local CA on each phone | a profile to install and trust by hand on every device | nothing: **rejected** for the same reason |
| **a per-install name under our domain** (the Plex approach) | nothing at all: `https://<id>.wilive.lxusbrain.com:47900` just works | a DNS zone we control and a small issuing service |

## The chosen design (pending the owner's go-ahead and accounts)

1. Each install makes a key pair and an id (the hash of its public key).
2. A small service run by us (a Cloudflare Worker on the free tier is enough) proves
   control of `*.<id>.wilive.lxusbrain.com` to Let's Encrypt by DNS-01, for a CSR the app
   sends, and returns the certificate. The service never sees the private key. It is
   rate-limited per id, and it only signs names under that id.
3. DNS: `<ip-with-dashes>.<id>.wilive.lxusbrain.com` resolves to that private address
   (a wildcard answered by the same Worker, or a fixed `A` record set by the app through
   the service). Private addresses in public DNS are what Plex does. Routers with DNS
   rebinding protection need the name allow-listed: the app detects that and falls back
   to http, saying why.
4. The app renews 30 days before expiry and serves https on the same port. The QR
   code and the address in the window move to the https name. Old http bookmarks
   redirect.
5. Then talk back: the viewer's microphone to the computer's speakers, behind its own
   switch in the owner's window, off by default.

## What it needs from the owner

- Which DNS provider holds `lxusbrain.com` (the website's CI deploys there), and an API
  token scoped to one subdomain (`wilive.lxusbrain.com`).
- A Cloudflare account (free) for the Worker, or another host the owner prefers.
- A yes on the cost: zero on free tiers at home scale; Let's Encrypt is free.
