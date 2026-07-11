# Amber Website

A self-contained, Amber-themed static website for the project. It provides:

- A landing page introducing Amber and Amber Pro.
- A downloads page for releases, installers, and deprecated source archives.
- A pricing/subscription page for Amber Pro, highlighting Groq LPU fast inference.

## Files

```
website/
├── index.html          # Landing page
├── downloads.html      # Release downloads
├── pricing.html        # Amber Pro pricing & subscription
├── css/
│   └── styles.css      # Shared amber-themed stylesheet
├── js/
│   ├── releases.js     # Loads and filters release data
│   └── pricing.js      # Handles subscription modal
├── data/
│   └── releases.json   # Release metadata and download links
└── README.md
```

## Local preview

Because the downloads page fetches `data/releases.json`, open the site through a local web server rather than directly from the filesystem:

```bash
cd website
python3 -m http.server 8080
```

Then visit http://localhost:8080.

## Updating releases

Edit `data/releases.json`. Each release can list installer and source assets. Set `deprecated: true` to move a release into the deprecated section with a warning.

## Deployment

The site is plain HTML/CSS/JS and can be hosted anywhere. A GitHub Actions workflow is included at `.github/workflows/deploy-website.yml` to publish the `website/` directory to GitHub Pages on pushes to `main`.

## Notes

- The subscription form on the pricing page is a front-end demo. Integrate it with your payment provider (e.g., Stripe) and backend before accepting real subscriptions.
- Download links point to the GitHub Releases page for `seriousaboutsolutions/amber`.
