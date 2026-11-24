# 0024: Guessing Bandcamp pages

Date: 24 November 2026. Status: accepted. Changes one line of [ADR 0017](0017-identification-providers-over-ureq.md).

## Context

ADR 0017 read Bandcamp only for a page the user pasted. Most edits and label exclusives in a club library are only on Bandcamp, so every one of them meant a trip to the browser. Bandcamp's `robots.txt` disallows `/search` and `/api/`, so asking Bandcamp to search is out.

## Decision

- When the earlier steps leave a track unsettled, identification tries up to 10 exact track addresses built from the file's own names: `https://<seller>.bandcamp.com/track/<slug>`. The slug is the full title with any mix in brackets, with and without the artist in front.
- Sellers are tried in this order: sellers learned from earlier Bandcamp matches for the same artist, remixer or mix maker, then the remixer, mix maker, artist and label as written, then a series name in the album, such as `night owl` from `night owl Vol.2`.
- Only the page's published structured data is read, at 1 request a second. A 404 is cached for 30 days like any other answer.
- A page whose `byArtist` names the seller and whose title has no `Artist - ` split is a label or shop page. Its artist is not proposed.
- Place names in the page's keywords are not proposed as genres.

## Why

An exact address is a plain page fetch that `robots.txt` allows, and it is what a user would type after guessing the same thing. Learned sellers make it better over time: after one pasted link from a seller, the rest of that seller's tracks in the library are found without help.

## What follows from it

- A miss costs up to 10 slow requests per track. They run last, only for tracks nothing else found.
- A seller whose URL differs from every name on the file is still only found from a pasted link.
