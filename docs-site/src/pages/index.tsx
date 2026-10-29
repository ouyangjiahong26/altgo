import React from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Translate, {translate} from '@docusaurus/Translate';
import {
  Mic,
  Infinity,
  Brain,
  Sparkles,
  Monitor,
  ClipboardCopy,
  ArrowRight,
  Download,
  BookOpen,
} from 'lucide-react';
import styles from './index.module.css';

export default function Home(): React.JSX.Element {
  const { siteConfig } = useDocusaurusContext();
  const [activeShot, setActiveShot] = React.useState(0);

  const features = [
    {
      icon: Monitor,
      title: translate({id: 'home.feature.platform.title', message: 'Four platform combinations'}),
      description: translate({
        id: 'home.feature.platform.description',
        message: 'Windows and Linux on x86_64 and arm64. The Linux side supports both X11 and Wayland.',
      }),
    },
    {
      icon: Mic,
      title: translate({id: 'home.feature.hold.title', message: 'Hold to record'}),
      description: translate({
        id: 'home.feature.hold.description',
        message: 'Hold right Alt to speak and release to transcribe. No window switching, no interruption.',
      }),
    },
    {
      icon: Infinity,
      title: translate({id: 'home.feature.continuous.title', message: 'Continuous mode'}),
      description: translate({
        id: 'home.feature.continuous.description',
        message: 'Double-press to start continuous recording for long speech or meetings. Single-press to stop.',
      }),
    },
    {
      icon: Brain,
      title: translate({id: 'home.feature.local.title', message: 'Local and online transcription'}),
      description: translate({
        id: 'home.feature.local.description',
        message: 'SenseVoice or FireRedASR2 CTC locally, even offline, or switch to online MiMo ASR.',
      }),
    },
    {
      icon: Sparkles,
      title: translate({id: 'home.feature.polish.title', message: 'LLM polishing'}),
      description: translate({
        id: 'home.feature.polish.description',
        message: 'Four polishing levels, with support for OpenAI, DeepSeek, Anthropic and Ollama.',
      }),
    },
    {
      icon: ClipboardCopy,
      title: translate({id: 'home.feature.clipboard.title', message: 'Clipboard and overlay'}),
      description: translate({
        id: 'home.feature.clipboard.description',
        message: 'The result goes to the clipboard and appears in the overlay. The full text stays available in history.',
      }),
    },
  ];

  const screenshots = [
    {src: '/altgo/img/screenshot-main.png', alt: translate({id: 'home.screenshot.main', message: 'Main window'})},
    {src: '/altgo/img/screenshot-home.png', alt: translate({id: 'home.screenshot.home', message: 'Home while transcribing'})},
    {src: '/altgo/img/screenshot-overlay.png', alt: translate({id: 'home.screenshot.overlay', message: 'Overlay'})},
    {src: '/altgo/img/screenshot-settings.png', alt: translate({id: 'home.screenshot.settings', message: 'Settings'})},
    {src: '/altgo/img/screenshot-history.png', alt: translate({id: 'home.screenshot.history', message: 'Transcription history'})},
  ];

  React.useEffect(() => {
    const timer = setInterval(() => {
      setActiveShot((prev) => (prev + 1) % screenshots.length);
    }, 4000);
    return () => clearInterval(timer);
  }, []);

  return (
    <Layout
      title={siteConfig.title}
      description={translate({id: 'home.meta.description', message: 'Speech to text, without typing'})}
    >
      {/* ─── 首屏 ─── */}
      <header className={styles.heroBanner}>
        <div className={styles.heroGlow} />
        <div className={styles.heroGrid} />
        <div className={clsx('container', styles.heroInner)}>
          <div className={styles.heroBadge}>
            <span className={styles.heroBadgeDot} />
            <Translate id="home.hero.badge">Windows and Linux, x86_64 and arm64</Translate>
          </div>
          <h1 className={styles.heroTitle}>
            <Translate id="home.hero.title.line1">No typing</Translate>
            <br />
            <span className={styles.heroTitleAccent}>
              <Translate id="home.hero.title.line2">Just speaking</Translate>
            </span>
          </h1>
          <p className={styles.heroTagline}>
            <Translate
              id="home.hero.tagline.line1"
              description="hero tagline, first line">
              Windows and Linux, x86_64 and arm64: four platform combinations.
            </Translate>
            <br />
            <Translate
              id="home.hero.tagline.line2"
              description="hero tagline, second line">
              Hold Alt to speak. On release, altgo transcribes with the engine you choose and can polish the text with an LLM.
            </Translate>
          </p>
          <div className={styles.buttons}>
            <Link
              className={clsx('button', styles.btnPrimary)}
              to="/docs/quick-start"
            >
              <Download size={18} />
              <Translate id="home.cta.quickStart">Quick Start</Translate>
            </Link>
            <Link
              className={clsx('button', styles.btnSecondary)}
              to="/docs/usage"
            >
              <BookOpen size={18} />
              <Translate id="home.cta.usage">Usage</Translate>
            </Link>
            <Link
              className={clsx('button', styles.btnGhost)}
              href="https://github.com/ouyangjiahong26/altgo"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/>
                <path d="M9 18c-4.51 2-5-2-7-2"/>
              </svg>
              GitHub
            </Link>
          </div>

          {/* 截图展示 */}
          <div className={styles.showcase}>
            <div className={styles.showcaseFrame}>
              {screenshots.map((s, i) => (
                <img
                  key={s.src}
                  src={s.src}
                  alt={s.alt}
                  className={clsx(
                    styles.showcaseImg,
                    i === activeShot && styles.showcaseImgActive
                  )}
                />
              ))}
              <div className={styles.showcaseOverlay} />
            </div>
            <div className={styles.showcaseDots}>
              {screenshots.map((_, i) => (
                <button
                  key={i}
                  className={clsx(
                    styles.showcaseDot,
                    i === activeShot && styles.showcaseDotActive
                  )}
                  onClick={() => setActiveShot(i)}
                  aria-label={translate(
                    {id: 'home.screenshot.viewN', message: 'View screenshot {n}'},
                    {n: i + 1},
                  )}
                />
              ))}
            </div>
          </div>
        </div>
      </header>

      <main>
        {/* ─── 工作原理 ─── */}
        <section className={styles.howItWorks}>
          <div className="container">
            <p className={styles.sectionKicker}>
              <Translate id="home.steps.kicker">Three steps</Translate>
            </p>
            <h2 className={styles.sectionTitle}>
              <Translate id="home.steps.title">Simple enough to skip the tutorial</Translate>
            </h2>
            <div className={styles.steps}>
              <div className={styles.step}>
                <div className={styles.stepNum}>1</div>
                <h3><Translate id="home.steps.install.title">Install and configure</Translate></h3>
                <p>
                  <Translate id="home.steps.install.description">
                    Download the deb, rpm or AppImage, then choose a local model or online transcription in Settings.
                  </Translate>
                </p>
              </div>
              <ArrowRight className={styles.stepArrow} size={24} />
              <div className={styles.step}>
                <div className={styles.stepNum}>2</div>
                <h3><Translate id="home.steps.speak.title">Hold Alt and speak</Translate></h3>
                <p>
                  <Translate id="home.steps.speak.description">
                    Hold right Alt to record and release to transcribe. Double-press for continuous mode.
                  </Translate>
                </p>
              </div>
              <ArrowRight className={styles.stepArrow} size={24} />
              <div className={styles.step}>
                <div className={styles.stepNum}>3</div>
                <h3><Translate id="home.steps.paste.title">Paste anywhere</Translate></h3>
                <p>
                  <Translate id="home.steps.paste.description">
                    The result is already on the clipboard. Confirm it in the overlay and paste it.
                  </Translate>
                </p>
              </div>
            </div>
          </div>
        </section>

        {/* ─── 功能 ─── */}
        <section className={styles.features}>
          <div className="container">
            <p className={styles.sectionKicker}>
              <Translate id="home.features.kicker">Features</Translate>
            </p>
            <h2 className={styles.sectionTitle}>
              <Translate id="home.features.title">Details designed for daily use</Translate>
            </h2>
            <div className={styles.featureGrid}>
              {features.map((f) => {
                const Icon = f.icon;
                return (
                  <div key={f.title} className={styles.featureCard}>
                    <div className={styles.featureIcon}>
                      <Icon size={22} strokeWidth={2} />
                    </div>
                    <h3>{f.title}</h3>
                    <p>{f.description}</p>
                  </div>
                );
              })}
            </div>
          </div>
        </section>

        {/* ─── 行动号召（CTA） ─── */}
        <section className={styles.cta}>
          <div className="container">
            <div className={styles.ctaBox}>
              <h2 className={styles.ctaTitle}>
                <Translate id="home.cta.finalTitle">Ready to speed up your typing?</Translate>
              </h2>
              <p className={styles.ctaSubtitle}>
                <Translate id="home.cta.finalSubtitle">
                  Install in 30 seconds and change how you type
                </Translate>
              </p>
              <div className={styles.buttons}>
                <Link
                  className={clsx('button', styles.btnPrimary)}
                  to="/docs/quick-start"
                >
                  <Translate id="home.cta.quickStart">Quick Start</Translate>{' '}
                  <ArrowRight size={18} />
                </Link>
                <Link
                  className={clsx('button', styles.btnGhost)}
                  href="https://github.com/ouyangjiahong26/altgo/releases"
                >
                  <Translate id="home.cta.download">Download latest</Translate>
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
