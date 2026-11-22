/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import { getRepoStats } from "@/lib/github";
import ThemeToggle from "./ThemeToggle";
import NavLinks from "./NavLinks";
import GitHubIcon from "./GitHubIcon";
import Logo from "./Logo";

export default async function Nav(): Promise<React.JSX.Element> {
  const stats = await getRepoStats();
  return (
    <nav className="fixed top-0 z-40 w-full border-b border-line/70 bg-bg/75 backdrop-blur-md">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-5 py-3">
        <a href="#top" aria-label="Chisle home">
          <Logo />
        </a>
        <div className="flex items-center gap-6 text-sm text-dim">
          <NavLinks />
        </div>
        <div className="flex items-center gap-2.5">
          <a
            href="https://github.com/JayPokale/Chisle"
            target="_blank"
            rel="noopener noreferrer"
            className="hidden items-center gap-1.5 rounded-md border border-line px-2.5 py-1.5 text-xs text-dim transition-colors hover:text-fg sm:flex"
          >
            <GitHubIcon size={14} />
            {stats.stars > 0 ? stats.stars.toLocaleString() : "GitHub"}
          </a>
          <ThemeToggle />
          <a
            href="#get-started"
            className="rounded-md bg-brand px-3.5 py-1.5 text-sm font-medium text-on-brand transition-colors hover:bg-brand-hover"
          >
            Get started
          </a>
        </div>
      </div>
    </nav>
  );
}
