/** 模型下载进度：一条细进度线加百分比。
 *
 * 下载在引擎步与润色步都会出现，抽成一个组件，免得两处各写一套宽度与配色。
 * 进度条是设计语言里唯一允许动画布局属性（width）的例外，其余动画只动
 * transform 与 opacity。 */
export function DownloadMeter({
  percent,
  connecting,
  t,
}: {
  percent: number;
  connecting: boolean;
  t: (key: string) => string;
}) {
  return (
    <div className="onboarding-meter">
      <div className="onboarding-meter-track">
        <div className="onboarding-meter-fill" style={{ width: `${percent}%` }} />
      </div>
      <span className="onboarding-meter-text">
        {connecting ? t("settings.model_download_connecting") : `${percent}%`}
      </span>
    </div>
  );
}
