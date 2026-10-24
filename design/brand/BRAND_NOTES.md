# OMCP reference app: brand concepts (draft, 2026-10-24)

The protocol keeps the name **OMCP**. These are candidate consumer names for the app. Nothing has been registered or pushed.

## How the checks were done (2026-10-24)
- Domains: RDAP via `https://rdap.org/domain/<d>` (registration date recorded). `dig` was not used, because the sandbox DNS answers 198.18.0.1 for every name. A 200 response with a date means the domain is registered. A 404 means RDAP has no registration record.
- App Store: iTunes Search API `https://itunes.apple.com/search?term=<n>&entity=software`, top 5 results.
- Google Play: `https://play.google.com/store/search?q=<n>&c=apps` scraped with curl. Results were not reliable (no ranked titles), so treat Play as **not checked**.
- GitHub: `https://api.github.com/users/<n>`. A 200 response means the login is taken.
- Trademarks (EUIPO/USPTO): **not checked** for any name. Only the ® marks found by web search are noted.
- Web search was done only for Meshu and Tuli. For the others it was **not checked** beyond the App Store results.

## Brainstorm
Hopla, Nidi, Meshu, Nodi, Relai, Nearo, Ripl, Meshly, Kinet, Tuli, Hoppa, Wisp, Lanto, Nodu, Ommi.

## Ranked shortlist
| # | Name | Rationale | .com | .app | GitHub | App Store top hits | Other |
|---|---|---|---|---|---|---|---|
| 1 | **Hopla** | A "hop" is a mesh hop. "Hop là!" is a friendly French exclamation, and it is easy to say in EN/FR/AR. | registered 2017-02-13 | registered 2018-05-09 | taken | "Hoopla: Your Library Anywhere" (big, confusingly similar), "Hopla Guide Strasbourg" | TM not checked; Hoopla similarity is a risk |
| 2 | **Nidi** | Italian for "nests": a cosy local cluster of peers. Two open syllables. | registered 1996-08-15 | registered 2023-10-31 | taken | NidiDrive, NidiDriver (minor) | TM not checked |
| 3 | **Meshu** | "Mesh" + a soft, friendly ending. The most literal of the names. | registered 2004-10-12 | **RDAP 404, no record found** (not verified with a registrar) | taken | none named Meshu in the top 5 | Web: Meshu® business-services mark (start.meshu.us), Meshu jewelry (meshu.io, 2012), meshU iOS app (2010) |
| 4 | Nodi | Nodes/knots (IT). | registered 2000-03-28 | registered 2026-01-18 | taken | "NODI App", "Nodi" | crowded |
| 5 | Relai | FR *relais*, a relay. | registered 2003-07-05 | registered 2021-02-11 | taken | "Relai World", "RELai" | crowded |
| 6 | Nearo | "Near" + o. | registered 2000-07-03 | registered 2025-04-20 | taken | **"Nearo: Offline Nearby Chat"**, a direct category conflict | avoid |

Rejected after the checks: Ripl (the "Ripl: Social Media Marketing" app exists), Meshly (3D/STL apps), Tuli (Finnish for "fire", already used as the Finnish name of FireChat, and the Tuli Health/tuli.com brands exist).

## Logo concepts (logos made for the top 3)
- **Hopla**: three nodes joined by two hop arcs. The last node and its arc are gateway blue, so it reads as "your message hops peer to peer, maybe out through a gateway."
- **Nidi**: a hexagonal "nest" of six peers around a blue centre node, all linked like a mesh. It stays a clear snowflake/hex shape at 29 px.
- **Meshu**: a chat bubble containing a three-node mesh triangle, showing messaging over a mesh.

None of the icons use text. Each has a light version (teal gradient) and a dark version (ink #0f1b2d with mint strokes). The wordmarks are lowercase, bold, set in the system font stack (`system-ui, -apple-system, Segoe UI, Roboto…`), and end with a teal "." as a node accent. They are live text, not paths, so outline them before final production.

## Colours (compatible with tokens.css)
- Mesh teal `#0b6e6a` (--brand), teal mid `#14807b` (--brand-mid), used for the icon gradient.
- Gateway blue `#1a5f9e` (--path-gateway). Inside the icon it is shown as the lighter tint `#8cc4f5` so it has contrast on teal.
- Ink `#0f1b2d` (--ink), the dark background and wordmark colour. Mint `#5fd3c9` is used for strokes on dark.

## Files
`<name>/icon.svg, icon-dark.svg, icon-1024.png, icon-180.png, icon-48.png, icon-dark-1024.png, wordmark(.svg/.png), wordmark-dark(.svg/.png), lockup.svg/.png, lockup-dark.svg`, plus `overview.png` and the generator script `gen.py`.
