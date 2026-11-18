"""Verify a local docs build or a hosted release and its downloads."""

from __future__ import annotations

import argparse
import json
from html.parser import HTMLParser
from http.client import HTTPResponse
from io import BytesIO
from pathlib import Path
from typing import cast
from urllib.parse import urljoin, urlsplit
from urllib.request import Request, urlopen
from zipfile import ZipFile

from sync_docs_reference import CHECKOUT, PUBLISHED

ARCHIVES = {
    "pilot.html": ("pilot", "variants/repaired.py"),
    "publish-provider.html": ("provider", "src/acme_storage/py.codepact"),
    "runtime-models.html": ("work", "src/main.py"),
    "concurrency.html": ("concurrency", "spawn/boundary.py"),
}


class Page(HTMLParser):
    def __init__(self, content: bytes) -> None:
        super().__init__()
        self.metadata: dict[str, str] = {}
        self.links: list[str] = []
        self.text: list[str] = []
        self.feed(content.decode("utf-8"))

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        fields = dict(attrs)
        if tag == "meta" and (name := fields.get("name")):
            self.metadata[name] = fields.get("content") or ""
        if tag == "a" and (link := fields.get("href")):
            self.links.append(link)

    def handle_data(self, data: str) -> None:
        self.text.append(data)


def read(location: str, relative: str) -> bytes:
    if urlsplit(location).scheme in {"http", "https"}:
        request = Request(
            urljoin(location.rstrip("/") + "/", relative),
            headers={"User-Agent": "CodePact docs release verification"},
        )
        response = cast(HTTPResponse, urlopen(request, timeout=15))
        with response:
            return response.read()
    return (Path(location) / relative).read_bytes()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def verify(
    location: str,
    docs_version: str,
    recipe_version: str,
    channel: str,
    revision: str | None,
    legacy: bool,
) -> None:
    pages = ("index.html", "install.html", "quickstart.html", "rules.html")
    guides = {name: Page(read(location, name)) for name in pages}
    quickstart = guides["quickstart.html"]
    require(
        f"codepact[cli]=={recipe_version}" in "".join(quickstart.text),
        f"Quick start does not install {recipe_version}",
    )
    for name, guide in guides.items():
        if not legacy:
            require(
                guide.metadata.get("codepact-docs-version") == docs_version
                and guide.metadata.get("codepact-docs-channel") == channel,
                f"{name} is missing the expected {channel} docs {docs_version} label",
            )
        if revision is not None:
            require(
                guide.metadata.get("codepact-docs-revision") == revision
                or (
                    legacy
                    and any(
                        f"github.com/eddiethedean/codepact/blob/{revision}/docs/"
                        in link
                        for link in guide.links
                    )
                ),
                f"{name} does not identify revision {revision}",
            )

    sample = next(link for link in quickstart.links if link.endswith("/lifecycle.py"))
    require(
        b"inspect_connection(conn)" in read(location, sample),
        "The first-check sample download is missing its borrowed call",
    )
    if not legacy:
        repaired = next(
            link for link in quickstart.links if link.endswith("/repaired.py")
        )
        require(
            read(location, repaired).rstrip().endswith(b"conn.close()"),
            "The repaired sample should end with cleanup",
        )
        for guide_name, (archive_name, member) in ARCHIVES.items():
            guide = Page(read(location, guide_name))
            link = f"_static/codepact-{archive_name}.zip"
            require(link in guide.links, f"{guide_name} is missing its archive link")
            with ZipFile(BytesIO(read(location, link))) as archive:
                require(
                    member in archive.namelist() and archive.testzip() is None,
                    f"Bad archive: {link}",
                )

    for version in (3, 4):
        schema = cast(
            "dict[str, dict[str, dict[str, int]]]",
            json.loads(read(location, f"check-report-v{version}.schema.json")),
        )
        require(
            schema["properties"]["report_schema_version"]["const"] == version,
            f"The report schema {version} download has the wrong version",
        )
    require(b"Search" in read(location, "search.html"), "Missing search page")
    index = read(location, "searchindex.js")
    require(
        b"Search.setIndex(" in index and b"quickstart" in index,
        "Missing quick start in the search index",
    )


class Arguments(argparse.Namespace):
    location: str
    version: str
    recipe_version: str
    channel: str
    revision: str | None
    legacy: bool


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument(
        "location", help="built HTML directory or hosted version URL"
    )
    _ = parser.add_argument("--version", default=CHECKOUT, help="expected docs version")
    _ = parser.add_argument("--recipe-version", default=PUBLISHED)
    _ = parser.add_argument(
        "--channel",
        choices=("Released", "Candidate"),
        default="Released" if CHECKOUT == PUBLISHED else "Candidate",
    )
    _ = parser.add_argument("--revision", help="expected exact hosted source commit")
    _ = parser.add_argument(
        "--legacy",
        action="store_true",
        help="verify a release before version metadata and ZIP downloads were added",
    )
    args = parser.parse_args(namespace=Arguments())
    if args.legacy and not args.revision:
        parser.error("--legacy requires --revision")
    verify(
        args.location,
        args.version,
        args.recipe_version,
        args.channel,
        args.revision,
        args.legacy,
    )
    print(f"Verified {args.channel.lower()} docs and downloads at {args.location}")


if __name__ == "__main__":
    main()
