/** 供应商目录加载 hook：设置页与引导向导共用。
 * 挂载时自动拉取一次，失败可重试（loadCatalog 失败即清缓存，重试会重新请求），
 * 加载中不重入；手填地址不依赖目录，失败不阻塞表单。 */
import { useEffect, useState } from "react";
import { loadCatalog } from "../config/catalog";
import type { ProviderPreset } from "../config/modelPresets";

export function useCatalogLoader() {
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  const loadCatalogPresets = async () => {
    if (catalogLoading) return;
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      setCatalogPresets(await loadCatalog());
    } catch (e) {
      setCatalogError(String(e));
    } finally {
      setCatalogLoading(false);
    }
  };

  useEffect(() => {
    void loadCatalogPresets();
  }, []);

  return { catalogPresets, catalogLoading, catalogError, loadCatalogPresets };
}
