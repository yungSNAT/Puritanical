// Live GitHub data, ISR-cached 1 hour, server-side so the numbers are in the
// HTML (SEO/LLM crawlers see them) and unauthenticated rate limits never bite.
const REPO = "JayPokale/Chisle";
const NO_STATS = { stars: 0, forks: 0 };

async function fetchRepoStats(): Promise<{ stars: number; forks: number }> {
  const res = await fetch(`https://api.github.com/repos/${REPO}`, {
    next: { revalidate: 3600 },
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!res.ok) throw new Error(String(res.status));
  const j = await res.json();
  return { stars: j.stargazers_count ?? 0, forks: j.forks_count ?? 0 };
}

// Any failure (network, status, bad JSON) shows the zero placeholder instead.
export async function getRepoStats(): Promise<{ stars: number; forks: number }> {
  return fetchRepoStats().catch(() => NO_STATS);
}
