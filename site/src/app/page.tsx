/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import Nav from "@/components/Nav";
import Hero from "@/components/Hero";
import GetStarted from "@/components/GetStarted";
import Agents from "@/components/Agents";
import Stats from "@/components/Stats";
import Axes from "@/components/Axes";
import Pipeline from "@/components/Pipeline";
import Compare from "@/components/Compare";
import Ladder from "@/components/Ladder";
import Terminal from "@/components/Terminal";
import Filters from "@/components/Filters";
import Benchmarks from "@/components/Benchmarks";
import Community from "@/components/Community";
import Faq from "@/components/Faq";
import Footer from "@/components/Footer";

export default function Page(): React.JSX.Element {
  return (
    <main>
      <Nav />
      <Hero />
      <Stats />
      <GetStarted />
      <Agents />
      <Axes />
      <Pipeline />
      <Terminal />
      <Filters />
      <Compare />
      <Ladder />
      <Benchmarks />
      <Faq />
      <Community />
      <Footer />
    </main>
  );
}
