// Safe core messages only. Provider response bodies are never translated here.
export const workflowErrors: Record<string, string> = {
  "全局提示词过长。": "The global prompt is too long.",
  "自定义提示词过长。": "The custom prompt is too long.",
  "应用已重新启动，操作已停止，可手动重新生成。":
    "The app restarted. This operation was stopped and can be regenerated manually.",
  "操作历史已达到容量限制；结果仅保留在当前会话。":
    "Operation history reached its capacity. This result remains in the current session only.",
  "操作内容过多，请清理旧操作后重试。":
    "Operation storage is full. Clear older operations and try again.",
  "请求标识无效。": "The request identifier is invalid.",
  "记录标识无效。": "The record identifier is invalid.",
  "原文或回复要点为空或过长。":
    "The source or reply intent is empty or too long.",
  "请选择有效的输出语言。": "Choose a valid output language.",
  "请先选择需要解析的原文。": "Select the source passage to explain first.",
  "原文选区已改变，请重新选择。":
    "The source selection changed. Select it again.",
  "选中的内容为空或过长。": "The selection is empty or too long.",
  "请先填写回复要点。": "Enter your reply intent first.",
  "请先选择回复语言。": "Choose a reply language first.",
  "上下文与回复要点合计过长，请缩小范围。":
    "The context and reply intent are too long together. Reduce their scope.",
  "请求无法编码。": "The request could not be encoded.",
  "请求标识已用于其他内容。":
    "This request identifier was already used for different content.",
  "正在处理的解析或回复过多，请稍候。":
    "Too many explanations or replies are running. Wait for one to finish.",
  "原操作已过期，请重新生成。":
    "The original operation expired. Generate again.",
  "原操作正在执行或不属于当前内容。":
    "The original operation is still running or belongs to different content.",
  "操作列表已满，请稍候。": "The operation list is full. Try again later.",
  "应用已重新启动，未自动重发请求。":
    "The app restarted. The request was not sent again automatically.",
  "原文已变化，请重新选择。": "The source changed. Select it again.",
  "请先处理原文的敏感内容。":
    "Resolve the source sensitive-content notice first.",
  "正文正在生成，请先停止或等待完成后再修改原文。":
    "The result is generating. Stop it or wait before editing the source.",
  "语境解析和回复需要模型服务，请先选择已配置的模型。":
    "Explanation and reply require a model service. Choose a configured model first.",
  "操作已清空或过期。": "This operation was cleared or expired.",
  "此操作没有等待放行的内容。":
    "This operation has no content waiting for permission.",
  "操作已清空。": "This operation was cleared.",
  "待确认内容已清空。": "The pending content was cleared.",
  "原文已变化，请重新生成并确认。":
    "The source changed. Generate again and confirm the new content.",
  "操作已过期。": "This operation expired.",
  "服务配置已变化，请重新生成。":
    "The service configuration changed. Generate again.",
  "请求等待超时，请重试。": "The request timed out while waiting. Try again.",
  "回复已生成，未获得完整回译参考。":
    "The reply is available, but the back-translation is incomplete.",
  "回复已生成，回译参考因并发限制未继续。":
    "The reply is available. Back-translation stopped at the concurrency limit.",
  "回复已生成，回译参考未完成。":
    "The reply is available. Back-translation did not finish.",
  "已停止。": "Stopped.",
  "没有可用的备用服务，或请求已超时。":
    "No eligible fallback service remains, or the request timed out.",
  "原文为空或超过 50,000 字符，请分段处理。":
    "The source is empty or exceeds 50,000 characters. Use smaller sections.",
  "截图数据无效或过大。": "The screenshot data is invalid or too large.",
  "截图像素超过限制，请缩小选区。":
    "The screenshot exceeds the pixel limit. Select a smaller area.",
  "原文已变化，请重新操作。": "The source changed. Try the action again.",
  "请先处理敏感内容。": "Resolve the sensitive-content notice first.",
  "检测到疑似敏感内容，原安全稿未改变。":
    "Possible sensitive content was detected. The existing safe source is unchanged.",
  "原文已变化或任务已删除，请重新操作。":
    "The source changed or the task was removed. Try the action again.",
  "没有可撤销的原文操作。": "There is no source change to undo.",
  "至少保留一个原文片段。": "Keep at least one source segment.",
  "编辑稿已改变，请先撤销编辑或恢复捕获原文。":
    "The edited draft changed. Undo the edit or restore the captured source first.",
  "未识别到可靠的基础表格，请对照原图编辑。":
    "No reliable basic table was detected. Compare with the screenshot and edit the text.",
  "本地分词范围无效。": "The local token range is invalid.",
  "本地匹配未完成。": "Local matching did not finish.",
  "原文已变化，请重新匹配。": "The source changed. Match it again.",
  "会话图片已达到 32 MiB 上限，原稿未改变。":
    "Session images reached the 32 MiB limit. The existing source is unchanged.",
  "正文正在生成，请先停止或等待完成后再追加。":
    "The result is generating. Stop it or wait before appending.",
  "每稿最多 10 段，原文未改变。":
    "Each draft supports up to 10 segments. The source is unchanged.",
  "待接收内容已满，请先处理已有片段。":
    "Pending captures are full. Resolve the existing captures first.",
  "待接收片段已清空或过期。": "The pending capture was cleared or expired.",
  "待接收片段已过期。": "The pending capture expired.",
  "无法创建原文草稿。": "The source draft could not be created.",
  "截图已不在本地。": "The screenshot is no longer stored locally.",
};
