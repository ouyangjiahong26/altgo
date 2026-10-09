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

export default function Home(): JSX.Element {
  const { siteConfig } = useDocusaurusContext();
  const [activeShot, setActiveShot] = React.useState(0);

  const features = [
    {
      icon: Monitor,
      title: translate({id: 'home.feature.platform.title', message: '四个平台组合全覆盖'}),
      description: translate({
        id: 'home.feature.platform.description',
        message: 'Windows 与 Linux、x86_64 与 arm64 同时支持，这样的语音转文字软件很少；Linux 侧兼容 X11 与 Wayland。',
      }),
    },
    {
      icon: Mic,
      title: translate({id: 'home.feature.hold.title', message: '长按即录'}),
      description: translate({
        id: 'home.feature.hold.description',
        message: '按住右 Alt 说话，松开即转写。无需切换窗口，不打断工作流。',
      }),
    },
    {
      icon: Infinity,
      title: translate({id: 'home.feature.continuous.title', message: '连续模式'}),
      description: translate({
        id: 'home.feature.continuous.description',
        message: '双击进入连续录音，适合长篇发言或会议记录。单击停止。',
      }),
    },
    {
      icon: Brain,
      title: translate({id: 'home.feature.local.title', message: '本地 SenseVoice 转写'}),
      description: translate({
        id: 'home.feature.local.description',
        message: 'SenseVoice 本地转写完全断网可用。',
      }),
    },
    {
      icon: Sparkles,
      title: translate({id: 'home.feature.polish.title', message: 'LLM 润色'}),
      description: translate({
        id: 'home.feature.polish.description',
        message: '四档润色强度，支持 OpenAI / DeepSeek / Anthropic / Ollama。',
      }),
    },
    {
      icon: ClipboardCopy,
      title: translate({id: 'home.feature.clipboard.title', message: '剪贴板 + 悬浮窗'}),
      description: translate({
        id: 'home.feature.clipboard.description',
        message: '转写成功后写入剪贴板并弹出悬浮窗。全文可在主窗历史中查看、复制。',
      }),
    },
  ];

  const screenshots = [
    {src: '/altgo/img/screenshot-main.png', alt: translate({id: 'home.screenshot.main', message: '主界面'})},
    {src: '/altgo/img/screenshot-home.png', alt: translate({id: 'home.screenshot.home', message: '首页转写中'})},
    {src: '/altgo/img/screenshot-overlay.png', alt: translate({id: 'home.screenshot.overlay', message: '悬浮窗'})},
    {src: '/altgo/img/screenshot-settings.png', alt: translate({id: 'home.screenshot.settings', message: '设置页面'})},
    {src: '/altgo/img/screenshot-history.png', alt: translate({id: 'home.screenshot.history', message: '转录历史'})},
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
      description={translate({id: 'home.meta.description', message: '无需打字，言出法随'})}
    >
      {/* ─── 首屏 ─── */}
      <header className={styles.heroBanner}>
        <div className={styles.heroGlow} />
        <div className={styles.heroGrid} />
        <div className={clsx('container', styles.heroInner)}>
          <div className={styles.heroBadge}>
            <span className={styles.heroBadgeDot} />
            <Translate id="home.hero.badge">Windows 与 Linux，x86_64 与 arm64</Translate>
          </div>
          <h1 className={styles.heroTitle}>
            <Translate id="home.hero.title.line1">无需打字</Translate>
            <br />
            <span className={styles.heroTitleAccent}>
              <Translate id="home.hero.title.line2">言出法随</Translate>
            </span>
          </h1>
          <p className={styles.heroTagline}>
            <Translate
              id="home.hero.tagline.line1"
              description="hero tagline, first line">
              同时支持 Windows 与 Linux、x86_64 与 arm64 四个组合的语音转文字软件，很少。
            </Translate>
            <br />
            <Translate
              id="home.hero.tagline.line2"
              description="hero tagline, second line">
              按住 Alt 说话，松开后在本地用 SenseVoice 转写，可接入 LLM 润色。
            </Translate>
          </p>
          <div className={styles.buttons}>
            <Link
              className={clsx('button', styles.btnPrimary)}
              to="/docs/quick-start"
            >
              <Download size={18} />
              <Translate id="home.cta.quickStart">快速开始</Translate>
            </Link>
            <Link
              className={clsx('button', styles.btnSecondary)}
              to="/docs/usage"
            >
              <BookOpen size={18} />
              <Translate id="home.cta.usage">使用说明</Translate>
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
                    {id: 'home.screenshot.viewN', message: '查看截图 {n}'},
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
              <Translate id="home.steps.kicker">三步上手</Translate>
            </p>
            <h2 className={styles.sectionTitle}>
              <Translate id="home.steps.title">简单到不需要教程</Translate>
            </h2>
            <div className={styles.steps}>
              <div className={styles.step}>
                <div className={styles.stepNum}>1</div>
                <h3><Translate id="home.steps.install.title">安装并设置</Translate></h3>
                <p>
                  <Translate id="home.steps.install.description">
                    下载 deb / rpm 安装包，在设置中下载 SenseVoice 模型。
                  </Translate>
                </p>
              </div>
              <ArrowRight className={styles.stepArrow} size={24} />
              <div className={styles.step}>
                <div className={styles.stepNum}>2</div>
                <h3><Translate id="home.steps.speak.title">按住 Alt 说话</Translate></h3>
                <p>
                  <Translate id="home.steps.speak.description">
                    长按右 Alt 开始录音，松开自动转写。双击进入连续模式。
                  </Translate>
                </p>
              </div>
              <ArrowRight className={styles.stepArrow} size={24} />
              <div className={styles.step}>
                <div className={styles.stepNum}>3</div>
                <h3><Translate id="home.steps.paste.title">粘贴使用</Translate></h3>
                <p>
                  <Translate id="home.steps.paste.description">
                    转写结果自动写入剪贴板，悬浮窗单行展示确认，直接粘贴使用。
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
              <Translate id="home.features.kicker">功能特点</Translate>
            </p>
            <h2 className={styles.sectionTitle}>
              <Translate id="home.features.title">为你设计的细节体验</Translate>
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
                <Translate id="home.cta.finalTitle">准备好提升输入效率了吗？</Translate>
              </h2>
              <p className={styles.ctaSubtitle}>
                <Translate id="home.cta.finalSubtitle">
                  30 秒安装，永久改变你的输入方式
                </Translate>
              </p>
              <div className={styles.buttons}>
                <Link
                  className={clsx('button', styles.btnPrimary)}
                  to="/docs/quick-start"
                >
                  <Translate id="home.cta.quickStart">快速开始</Translate>{' '}
                  <ArrowRight size={18} />
                </Link>
                <Link
                  className={clsx('button', styles.btnGhost)}
                  href="https://github.com/ouyangjiahong26/altgo/releases"
                >
                  <Translate id="home.cta.download">下载最新版</Translate>
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
