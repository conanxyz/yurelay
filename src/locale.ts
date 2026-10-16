import type { Settings } from "./types";
export type Locale = "zh" | "en";
export function localeFor(settings: Settings): Locale {
  if (settings.ui_language === "zh" || settings.ui_language === "en")
    return settings.ui_language;
  return /^(zh|简体中文|繁體中文|中文|Chinese)/i.test(settings.target_language)
    ? "zh"
    : "en";
}
export const text = (locale: Locale, zh: string, en: string) =>
  locale === "zh" ? zh : en;
export function applyAppearance(settings: Settings) {
  const locale = localeFor(settings);
  document.documentElement.lang = locale === "zh" ? "zh-Hans" : "en";
  document.documentElement.dataset.theme = settings.theme;
  document.documentElement.style.setProperty(
    "--ui-size",
    `${settings.ui_font_size}px`,
  );
}
export const languages = [
  ["zh-Hans", "简体中文", "Simplified Chinese"],
  ["zh-Hant", "繁體中文", "Traditional Chinese"],
  ["en", "英语", "English"],
  ["ja", "日语", "Japanese"],
  ["ko", "韩语", "Korean"],
  ["fr", "法语", "French"],
  ["de", "德语", "German"],
  ["es", "西班牙语", "Spanish"],
  ["pt", "葡萄牙语", "Portuguese"],
  ["ru", "俄语", "Russian"],
  ["ar", "阿拉伯语", "Arabic"],
];
export function primaryCode(value: string) {
  return (
    (
      {
        简体中文: "zh-Hans",
        繁體中文: "zh-Hant",
        中文: "zh-Hans",
        English: "en",
        英语: "en",
      } as Record<string, string>
    )[value] ?? value
  );
}
export function languageName(value: string, locale: Locale) {
  const row = languages.find((row) => row[0] === primaryCode(value));
  if (row) return row[locale === "zh" ? 1 : 2];
  try {
    return (
      new Intl.DisplayNames([locale === "zh" ? "zh-Hans" : "en"], {
        type: "language",
      }).of(value) ?? value
    );
  } catch {
    return value;
  }
}
const statuses: Record<string, [string, string]> = {
  capturing: ["正在取词", "Capturing"],
  recognizing: ["正在识别", "Recognizing"],
  queued: ["等待翻译", "Queued"],
  translating: ["正在翻译", "Translating"],
  completed: ["完成", "Completed"],
  original_only: ["仅显示原文", "Original only"],
  ready: ["待翻译", "Ready to translate"],
  failed: ["失败", "Failed"],
  stopped: ["已停止", "Stopped"],
  interrupted: ["已中断", "Interrupted"],
};
export function statusName(value: string, locale: Locale) {
  return statuses[value]?.[locale === "zh" ? 0 : 1] ?? value;
}
const errors: Record<string, string> = {
  "请输入有效的服务地址。": "Enter a valid service URL.",
  "请填写或选择模型。": "Enter or choose a model.",
  "两个快捷键不能相同。": "The two shortcuts must be different.",
  "请使用 HTTPS；仅本机回环地址支持 HTTP。":
    "Use HTTPS. HTTP is only supported for local loopback addresses.",
  "这看起来是完整请求地址，请选择“完整地址”。":
    "This looks like a complete endpoint. Choose “Full request URL”.",
  "记录已删除或到期。": "This record has been deleted or has expired.",
  "没有可翻译的文字。": "No text is available to translate.",
  "原文为空或过长。": "The source text is empty or too long.",
  "模型没有返回译文。": "The model did not return a translation.",
  "原服务配置已删除，请重新选词或截图。":
    "The original service was removed. Capture the text again.",
  "这条记录使用的服务配置已改变，请复制原文重新翻译。":
    "The service configuration changed. Copy the source text to start a new translation.",
  "无法读取系统钥匙串，请解锁钥匙串或重新保存密钥。":
    "Unable to read the system keychain. Unlock it or save the key again.",
  "请先在设置中开启 Yurelay 的屏幕录制权限。":
    "Enable Screen Recording for Yurelay in Settings.",
  "没有读取到选中文字，请重新选择或使用截图。":
    "No selected text was found. Select text again or capture a screenshot.",
  "本地文字识别失败，请缩小选区后重试。":
    "Local text recognition failed. Try a smaller selection.",
  "选区中没有识别到文字。": "No text was detected in the selected area.",
  "应用正在退出。": "The application is quitting.",
};
Object.assign(errors, {
  "请先配置此服务的 API Key 或应用密钥。":
    "Configure this service's API key or application secret first.",
  "请先配置此服务的 API Key。": "Configure this service's API key first.",
  "请填写有道应用 ID。": "Enter the Youdao application ID.",
  "有道请求失败，请检查应用 ID、密钥、服务开通状态和额度。":
    "Youdao could not complete the request. Check the application ID, secret, enabled services and quota.",
  "本机词典未找到释义，请在 macOS 词典中启用所需词典。":
    "No definition was found. Enable the dictionaries you need in the macOS Dictionary app.",
  "系统词典用于查询单词或短语，请缩短原文后重试。":
    "The system dictionary looks up words and phrases. Shorten the source and try again.",
  "系统词典查询失败。": "The system dictionary lookup failed.",
  "查询的生词过长。": "A vocabulary lookup term is too long.",
  "此平台暂不支持系统词典。":
    "The system dictionary is not available on this platform.",
  "此服务不使用模型列表。": "This service does not use a model list.",
  "Claude 返回流式错误，请检查模型、额度或稍后重试。":
    "Claude reported a streaming error. Check the model and quota, or try again later.",

  "接口返回了无效的 UTF-8 内容。": "The service returned invalid UTF-8 text.",
  "接口返回的流式数据不是有效 JSON。": "The stream contains invalid JSON.",
  "模型服务在流式响应中返回错误，请检查服务配置或稍后重试。":
    "The service reported an error in the stream. Check the service settings or try again later.",
  "流式响应的单个事件过大。": "A streamed event exceeded the size limit.",
  "保存天数必须介于 1 和 36500 之间。":
    "Retention must be between 1 and 36500 days.",
  "同时翻译任务数须为 1 或 2。": "Concurrent translations must be 1 or 2.",
  "字号须介于 12 和 28 之间。": "Reading font size must be between 12 and 28.",
  "服务名称、地址或模型名称过长。":
    "The service name, endpoint, or model name is too long.",
  "服务地址不能包含用户名、密码或片段；请在 API Key 字段设置密钥。":
    "The endpoint cannot contain credentials or a fragment. Use the API Key field instead.",
  "服务数量须介于 1 和 20 之间。": "Keep between 1 and 20 services.",
  "服务标识无效或重复。": "A service identifier is invalid or duplicated.",
  "此完整地址无法推导模型列表地址，请手动填写模型名称。":
    "A model-list URL cannot be derived from this endpoint. Enter a model name manually.",
  "温度必须介于 0 和 2 之间。": "Temperature must be between 0 and 2.",
  "界面语言、主题或字号无效。":
    "Invalid interface language, theme, or font size.",
  "请先选择翻译服务。": "Choose a translation service first.",
  "请选择有效的目标语言。": "Choose a valid target language.",
  "原文过长，请分段翻译。":
    "The source is too long. Translate it in smaller sections.",
  "当前会话内容已达到内存上限，请先处理未保存的结果。":
    "This session has reached its memory limit. Save or remove pending results first.",
  "更正后的原文为空或过长。": "The edited source is empty or too long.",
  "未保存的结果较多，请先重试保存或在设置中关闭本地保存。":
    "Too many results are waiting to be saved. Retry saving or turn off local storage.",
  "等待中的任务已满。请等待一个任务完成，或主动停止不再需要的任务。":
    "The queue is full. Wait for a task to finish or stop an unwanted task.",
  "等待中的任务已满，请稍后重试。": "The queue is full. Try again later.",
  "译文超过大小限制，已停止接收。":
    "The translation exceeded the size limit. Receiving has stopped.",
  "退出时任务尚未完成，可手动重试。":
    "The task was unfinished when the app quit. Retry it manually.",
  "API Key 包含无效字符。": "The API key contains invalid characters.",
  "响应过大，已停止接收。":
    "The response exceeded the size limit. Receiving has stopped.",
  "已停止翻译。": "Translation stopped.",
  "无法连接翻译服务，请检查地址和网络。":
    "Cannot connect to the translation service. Check the endpoint and your connection.",
  "服务响应不是有效 JSON。": "The service response is not valid JSON.",
  "服务未提供兼容的模型列表，可直接手动填写模型。":
    "This service does not provide a compatible model list. Enter the model manually.",
  "模型列表不是有效 JSON，可直接手动填写模型。":
    "The model list is not valid JSON. Enter the model manually.",
  "模型因输出长度限制停止，已保留部分译文。":
    "The model reached its output limit. The partial translation has been kept.",
  "模型未正常完成文本翻译，已保留收到的内容。":
    "The model did not finish normally. Received text has been kept.",
  "此接口未返回流式数据。请在服务设置中关闭流式后重试。":
    "The endpoint did not return a stream. Turn off streaming for this service and try again.",
  "等待首段译文超时，服务可能正在排队或推理。":
    "Timed out waiting for the first translation. The service may be queued or reasoning.",
  "翻译流长时间没有继续返回，已停止等待。":
    "The translation stream stopped responding. Waiting has ended.",
  "请求超时，请稍后重试。": "The request timed out. Try again later.",
  "连接提前结束，已保留部分译文，可手动重试。":
    "The connection ended early. The partial translation has been kept; you can retry.",
  "上次退出时任务尚未完成，可手动重试。":
    "The task was unfinished when the app last quit. You can retry it.",
  "历史筛选条件无效。": "Invalid history filters.",
  "截图数据无效或过大。": "The screenshot data is invalid or too large.",
  "截图文件过大。": "The screenshot file is too large.",
  "无法读写本地数据，请检查磁盘空间和目录权限。":
    "Cannot read or write local data. Check free disk space and folder permissions.",
  "本地设置无法解析；数据未被覆盖。":
    "Local settings could not be read. Existing data has not been overwritten.",
  "记录标识无效。": "Invalid record identifier.",
  "部分历史记录无法解析，数据未被覆盖。":
    "Some history records could not be read. Existing data has not been overwritten.",
  "API Key 过长或包含换行。":
    "The API key is too long or contains a line break.",
  "原始截图已不在内存中，请重新截图。":
    "The original screenshot is no longer in memory. Capture it again.",
  "原密钥无法读取，暂不能安全替换。请先解锁系统钥匙串并重启应用。":
    "The original key cannot be read or safely replaced. Unlock the keychain and restart the app.",
  "密钥对应的服务不存在。": "The service for this key no longer exists.",
  "截图超过会话图片缓存上限；已尝试单独保存到本地。":
    "The screenshot exceeds the session cache limit. A separate local save was attempted.",
  "本地保存尚未确认，请稍后重试保存。":
    "Local saving is not yet confirmed. Retry saving shortly.",
  "此任务已不在等待队列。": "This task is no longer queued.",
  "此任务正在执行，或缺少可翻译原文。":
    "This task is running or has no source text to translate.",
  "系统凭据操作失败。": "The system credential operation failed.",
  "系统凭据更新和回滚均未完成，请检查钥匙串后重新配置。":
    "Credential update and rollback did not complete. Check the keychain and configure the key again.",
  "设置保存失败，密钥回滚也失败，请重新设置密钥。":
    "Saving settings and restoring the previous key failed. Configure the key again.",
  "读取系统凭据失败。": "Could not read system credentials.",
  "这条结果仅保留在本次会话。":
    "This result is only kept for the current session.",
  "进行中的截图已占满会话缓存，请稍后重试。":
    "Pending screenshots have filled the session cache. Try again later.",
  "复制失败。": "Copy failed.",
  "文字过长。": "The text is too long.",
  "无法展开窗口。": "Could not expand the window.",
  "无法恢复小窗口。": "Could not restore the compact window.",
  "无法改变窗口置顶状态。": "Could not change the window pin state.",
  "无法读取窗口状态。": "Could not read the window state.",
  "无法调整窗口大小。": "Could not resize the window.",
  "无法连接结果窗口。": "Could not connect to the results window.",
  "无法退出全屏。": "Could not leave full screen.",
  "窗口不可用。": "The window is unavailable.",
  "划词快捷键格式无效，例如 Alt+D。": "Invalid selection shortcut. Try Alt+D.",
  "快捷键至少需要 Option/Alt、Control 或 Command 修饰键。":
    "A shortcut needs an Option/Alt, Control, or Command modifier.",
  "截图快捷键格式无效，例如 Alt+S。": "Invalid screenshot shortcut. Try Alt+S.",
  "无法更新快捷键，请重启应用。":
    "Could not update shortcuts. Restart the app.",
  "截图过大，请缩小选区。":
    "The screenshot is too large. Select a smaller region.",
  文字包含无效字符: "The text contains invalid characters.",
  "无法更新系统钥匙串；设置未保存。":
    "Could not update the system keychain. Settings were not saved.",
  未知权限类型: "Unknown permission type.",
  "此平台的取词与截图适配尚未实现。":
    "Text selection and screenshots are not yet available on this platform.",
  "此平台的系统凭据存储尚未实现。":
    "System credential storage is not yet available on this platform.",
  "此平台适配尚未实现。": "This platform is not yet supported.",
  "钥匙串中的密钥格式无效，请重新设置。":
    "The keychain key has an invalid format. Set the key again.",
  "剪贴板含延迟数据，已保留剪贴板。请使用截图翻译。":
    "The clipboard contains deferred data and was preserved. Use screenshot translation.",
  "剪贴板或前台应用已改变，请重试。":
    "The clipboard or foreground app changed. Try again.",
  "取词时前台应用已改变。": "The foreground app changed during capture.",
  "取词时前台应用已改变，请重新选择。":
    "The foreground app changed. Select the text again.",
  "密码输入框不支持取词。": "Text cannot be captured from password fields.",
  "屏幕图像已失效，请重新截图。":
    "The screen image is no longer valid. Capture it again.",
  "屏幕截图失败，请检查权限后重试。":
    "Screen capture failed. Check permissions and try again.",
  "当前剪贴板无法安全备份，请使用截图翻译。":
    "The clipboard cannot be backed up safely. Use screenshot translation.",
  "无法读取当前剪贴板，已保留原内容。请重试或使用截图翻译。":
    "The clipboard could not be read and was preserved. Try again or use screenshot translation.",
  "剪贴板部分格式无法读取，已保留原内容。请使用截图翻译。":
    "Some clipboard formats could not be read. The clipboard was preserved. Use screenshot translation.",
  "剪贴板内容超过 8 MiB，已保留原内容。请使用截图翻译。":
    "The clipboard exceeds the 8 MiB backup limit and was preserved. Use screenshot translation.",
  "截图编码失败。": "Screenshot encoding failed.",
  "无法发送复制快捷键，请使用截图。":
    "Could not send the copy shortcut. Use a screenshot.",
  "无法安全监测复制操作，请使用截图翻译。":
    "The copy operation cannot be monitored safely. Use screenshot translation.",
  "无法截取该区域，请重试。": "Could not capture that region. Try again.",
  "无法读取屏幕，请检查屏幕录制权限。":
    "Could not read the screen. Check Screen Recording permission.",
  "检测到新的操作，已停止取词并保留当前剪贴板。":
    "New input was detected. Capture stopped and the clipboard was preserved.",
  "正在截图，请完成或取消当前选区。":
    "A screenshot is in progress. Finish or cancel the selection.",
  "正在读取选中文字，请稍候。": "Reading selected text. Please wait.",
  "该应用未提供选中文字。请使用截图翻译。":
    "This app did not provide selected text. Use screenshot translation.",
  "请先在设置中开启 Yurelay 的辅助功能权限。":
    "Enable Accessibility permission for Yurelay in Settings.",
  "请在其他应用中选中文字，再按划词快捷键。":
    "Select text in another app, then press the selection shortcut.",
  "请松开快捷键后重试，或使用截图。":
    "Release the shortcut keys and try again, or use a screenshot.",
  "选区过大，请重新选择较小区域。":
    "The selection is too large. Select a smaller region.",
});
export function errorText(value: unknown, locale: Locale): string {
  const message = String(value ?? "");
  if (locale === "zh") return message;
  if (errors[message]) return errors[message];
  if (message === "智谱温度必须介于 0 和 1 之间。")
    return "Zhipu temperature must be between 0 and 1.";
  if (message.startsWith("请先配置翻译服务："))
    return (
      "Configure a translation service first: " +
      errorText(message.slice("请先配置翻译服务：".length), locale)
    );
  const http = message.match(/^翻译服务返回 HTTP (\d+)。/);
  if (http)
    return `The translation service returned HTTP ${http[1]}. ${http[1] === "400" ? "Check the model name and request parameters." : http[1] === "401" ? "Check the API key for this service." : http[1] === "403" ? "This account or key does not have access." : http[1] === "429" ? "Check the provider rate limit and quota." : "Check the service configuration or try again later."}`;
  const zhipu = message.match(/^智谱请求失败（HTTP (\d+)，错误码 (\d+)）：/);
  if (zhipu) {
    const reasons: Record<string, string> = {
      1210: "Invalid request parameters.",
      1211: "Model does not exist or is unavailable on this endpoint.",
      1212: "The model does not support this call method.",
      1213: "A required parameter is missing.",
      1214: "A parameter is invalid for this model.",
      1215: "Some parameters cannot be used together.",
      1220: "This key does not have endpoint access.",
      1221: "The endpoint has been retired.",
      1222: "The endpoint does not exist.",
      1261: "The prompt exceeds the model limit.",
      1301: "The provider content filter rejected the request.",
      1311: "The subscription does not include this model.",
      1315: "This key is restricted to enterprise Coding Plan use.",
    };
    const detail = message.split("服务端详情：")[1];
    return `Zhipu request failed (HTTP ${zhipu[1]}, code ${zhipu[2]}). ${reasons[zhipu[2]] ?? "Check this code in the provider console or error documentation."}${detail ? ` Provider detail: ${detail}` : ""}`;
  }
  const shortcut = message.match(/^快捷键 (.+) 无法注册/);
  if (shortcut)
    return `Shortcut ${shortcut[1]} could not be registered. Another app may be using it.`;
  if (!/[\u3400-\u9fff]/.test(message)) return message;
  return `The operation could not be completed. ${message}`;
}
