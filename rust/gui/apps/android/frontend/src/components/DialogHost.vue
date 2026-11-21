<script setup lang="ts">
import { computed } from "vue";
import IconGlyph from "./IconGlyph.vue";
import {
  beginScopeDownload,
  cancelDownload,
  clearCache,
  closeDialog,
  dialogCardStyle,
  formatSize,
  isDark,
  isLandscape,
  saveCookie,
  saveLiveChoice,
  state,
} from "../store";

const helpFormats = [
  "5.12 复制打开抖音，看看【赵今麦angel的作品】小林～拍照📷# 剧剧抖有戏  https://v.douyin.com/HUbJB4-GQdc/ 03/26 e@b.nD yGi:/ ",
  "https://v.douyin.com/L8tSCYe-sC4",
  "https://www.douyin.com/video/7588361322312650038",
  "https://m.douyin.com/share/video/7588361322312650038",
  "https://www.iesdouyin.com/share/video/7588361322312650038",
  "https://www.douyin.com/jingxuan?modal_id=7701193143558901018",
  "https://www.douyin.com/jingxuan?modal_id=7674934407219793171",
];

const xhsFormats = [
  "75 【新巡演，整起！^_^ - 张杰 | 小红书 - 你的生活兴趣社区】 😆 RC8p7DRyJZO8KBy 😆 https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
  "https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
  "bloc1✖️愤怒攀岩👊😡👊 今天怒气值max，状态和能力... https://xhslink.cn/o/8C9r0KXJsjj 把口令复制下来，进入【小红书】看笔记~",
  "https://www.xiaohongshu.com/explore/6644e3a8000000001e01cc4d?xsec_token=ABxxxx",
  "https://www.xiaohongshu.com/user/profile/65c5fd5600000000110173a5/6773508e0000000009015fa0?xsec_token=ABxxxx",
];

const download = computed(() => state.download);
const allCompleted = computed(
  () => Boolean(download.value && download.value.total > 0 && download.value.completed === download.value.total),
);
const cookieCompact = computed(
  () =>
    state.dialog === "cookie" &&
    isLandscape.value &&
    (state.insets.imeBottom || 0) > 96,
);
const overlayStyle = computed(() => {
  if (state.dialog !== "cookie" || isLandscape.value) return {};
  return {
    paddingBottom: `${Math.max(0, state.insets.imeBottom || 0)}px`,
  };
});
const cookieInputStyle = computed(() => {
  if (cookieCompact.value) {
    const height = Math.max(
      38,
      state.viewportHeight -
        (state.insets.top || 0) -
        (state.insets.bottom || 0) -
        (state.insets.imeBottom || 0) -
        8,
    );
    return {
      height: `${height}px`,
      maxHeight: `${height}px`,
    };
  }
  return {};
});
const cardStyle = computed<Record<string, string>>(() => {
  if (!cookieCompact.value) return dialogCardStyle.value;
  const areaHeight = Math.max(
    1,
    state.viewportHeight - (state.insets.imeBottom || 0),
  );
  return {
    width: `${Math.max(1, state.viewportWidth)}px`,
    maxWidth: "none",
    height: `${areaHeight}px`,
    maxHeight: `${areaHeight}px`,
    padding: `${state.insets.top || 0}px ${
      (state.insets.right || 0) + 16
    }px ${(state.insets.bottom || 0) + 8}px ${
      (state.insets.left || 0) + 16
    }px`,
    borderRadius: "0",
    background: "transparent",
    gap: "0",
    overflow: "visible",
  };
});

function actionWidth(label: string): string {
  const width = [...label].reduce(
    (sum, character) => sum + (character.charCodeAt(0) < 128 ? 8 : 14),
    0,
  );
  return `${Math.max(72, width + 40)}px`;
}

function onOverlayPointerDown(event: PointerEvent): void {
  event.preventDefault();
  event.stopPropagation();
}
</script>

<template>
  <div
    class="dialog-overlay"
    :class="{ 'cookie-dialog-overlay': state.dialog === 'cookie', 'landscape': isLandscape, 'cookie-compact': cookieCompact }"
    :style="overlayStyle"
    @pointerdown.self="onOverlayPointerDown"
  >
    <div
      class="dialog-card"
      :class="{ 'cookie-compact-card': cookieCompact }"
      :style="cardStyle"
      @pointerdown.stop
      @pointerup.stop
    >
      <template v-if="state.dialog === 'help'">
        <div class="dialog-title" style="font-size: 18px">使用说明</div>
        <div class="dialog-content help-content">
          <div class="help-section">
            <div class="help-lead">支持解析抖音以下链接格式：</div>
            <div
              v-for="(format, index) in helpFormats"
              :key="`d-${index}`"
              class="help-format-row"
            >
              <IconGlyph
                name="CHECK_CIRCLE"
                :size="16"
                mono
                color="#05c001"
                class="help-check"
              />
              <span class="help-format">{{ format }}</span>
            </div>
          </div>
          <div class="help-section">
            <div class="help-lead">
              支持解析小红书笔记（视频 / 图文），以下格式均支持：
            </div>
            <div
              v-for="(format, index) in xhsFormats"
              :key="`x-${index}`"
              class="help-format-row"
            >
              <IconGlyph
                name="CHECK_CIRCLE"
                :size="16"
                mono
                color="#05c001"
                class="help-check"
              />
              <span class="help-format">{{ format }}</span>
            </div>
          </div>
          <div class="help-note">
            <IconGlyph name="LINK" :size="16" class="help-link-icon" />
            <span class="help-note-text">
              支持批量粘贴多个抖音 / 小红书分享链接，系统会自动提取所有有效链接。
            </span>
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('知道了') }"
            @click="closeDialog"
          >
            知道了
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'about'">
        <div class="dialog-title" style="font-size: 24px">关于</div>
        <div class="dialog-content about-content">
          <div class="about-product">短视频解析工具</div>
          <div class="about-line">版本: {{ state.version }}</div>
          <div class="about-line">功能: 解析抖音、小红书链接，获取下载地址</div>
          <div class="about-line">作者: 梦丶随心飞 / Dreamfly</div>
          <div class="about-line">邮箱: dfvips@qq.com</div>
          <div class="about-line">
            声明: 本工具仅用于学习和研究视频网站防盗链等安全技术，免费无广告，请勿用于任何商业或非法用途，如产生法律纠纷与本人无关；如果侵犯了您的权益，请通过邮箱联系本人，本人愿意提供视频网站/应用安全性等相关技术支持；私自修改、传播以及利用此工具牟利等行为，后果由修改者/传播者/牟利者承担；使用此工具一切后果由使用者负责；
            <br />
            使用此工具即代表您同意此声明。
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('确定') }"
            @click="closeDialog"
          >
            确定
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'cookie'">
        <div class="dialog-title" style="font-size: 22px">Cookie 设置</div>
        <div class="dialog-content cookie-content">
          <div class="cookie-hint">可粘贴浏览器中小红书站点的 Cookie</div>
          <textarea
            id="xhs-cookie-input"
            v-model="state.cookieDraft"
            class="cookie-input"
            :class="{ error: state.cookieError }"
            :style="cookieInputStyle"
            placeholder="请输入小红书 Cookie，如 loadts=xxx;xsecappid=xhs-pc-web;acw_tc=xxxx;x-rednote-datactry=CN;webBuild=6.61.6;sec_poison_id=xxxx;web_session=xxxxx;ets=xxxx;id_token=xxxx;webId=xxxx;x-rednote-holderctry=CN"
            spellcheck="false"
          />
          <div v-if="state.cookieError" class="cookie-error">Cookie 格式不正确</div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button plain-action dialog-action"
            type="button"
            :style="{ width: actionWidth('取消') }"
            @pointerdown.stop
            @click.stop="closeDialog"
          >
            取消
          </button>
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('保存') }"
            @pointerdown.stop
            @click.stop="saveCookie"
          >
            保存
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'live'">
        <div class="live-dialog-header">
          <div class="dialog-title" style="font-size: 20px">导出格式</div>
          <div class="live-dialog-subtitle">选择默认的导出方式</div>
        </div>
        <div class="live-options">
          <button
            class="live-option"
            type="button"
            @click="state.liveChoice = false"
          >
            <span class="live-option-copy">
              <span class="live-option-title">普通视频 / 图片</span>
              <span class="live-option-subtitle">按照原始格式保存</span>
            </span>
            <span
              class="live-checkbox"
              :class="{ checked: !state.liveChoice }"
            >
              <IconGlyph
                v-if="!state.liveChoice"
                name="CHECK_MARK"
                :size="14"
                mono
                color="#ffffff"
              />
            </span>
          </button>
          <button
            class="live-option"
            type="button"
            @click="state.liveChoice = true"
          >
            <span class="live-option-copy">
              <span class="live-option-title">实况照片（Motion Photo）</span>
              <span class="live-option-subtitle">
                保存为谷歌标准 Motion Photo（JPG 内嵌视频）
              </span>
            </span>
            <span
              class="live-checkbox"
              :class="{ checked: state.liveChoice }"
            >
              <IconGlyph
                v-if="state.liveChoice"
                name="CHECK_MARK"
                :size="14"
                mono
                color="#ffffff"
              />
            </span>
          </button>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button plain-action dialog-action"
            type="button"
            :style="{ width: actionWidth('取消') }"
            @click="closeDialog"
          >
            取消
          </button>
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('确定') }"
            @click="saveLiveChoice"
          >
            确定
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'cache'">
        <div class="dialog-title" style="font-size: 20px">清除缓存</div>
        <div class="dialog-content cache-content">
          <div class="cache-question">确定要清除所有缓存数据吗？</div>
          <div class="cache-size">当前缓存大小: {{ formatSize(state.cacheInfo.size) }}</div>
          <div class="cache-path-label">将清理以下位置的缓存:</div>
          <div class="cache-path">• {{ state.cacheInfo.path }}</div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button plain-action dialog-action"
            type="button"
            :style="{ width: actionWidth('取消') }"
            @click="closeDialog"
          >
            取消
          </button>
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('清理') }"
            :disabled="state.dialogBusy"
            @click="clearCache"
          >
            清理
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'no-cache'">
        <div class="dialog-title" style="font-size: 20px">暂无缓存</div>
        <div class="dialog-content">
          <div class="dialog-secondary-text">
            当前没有可清理的缓存，无需清理
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('知道了') }"
            @click="closeDialog"
          >
            知道了
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'cache-result'">
        <div class="dialog-result-title">
          <IconGlyph
            :name="state.clearResult.success ? 'CHECK_CIRCLE' : 'INFO'"
            :size="24"
            mono
            :color="state.clearResult.success ? '#05c001' : '#d32f2f'"
          />
          <span>{{ state.clearResult.success ? "清理完成" : "清理失败" }}</span>
        </div>
        <div class="dialog-content cache-result-content">
          <div
            class="cache-result-message"
            :class="{ error: !state.clearResult.success }"
          >
            {{ state.clearResult.message }}
          </div>
          <div
            v-for="(detail, index) in state.clearResult.details"
            :key="index"
            class="cache-result-detail"
          >
            {{ detail }}
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('确定') }"
            @click="closeDialog"
          >
            确定
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'download-selection'">
        <div class="dialog-title-row">
          <div class="dialog-title" style="font-size: 20px">下载资源</div>
          <button
            class="dialog-close-button"
            type="button"
            aria-label="关闭"
            @click="closeDialog"
          >
            <IconGlyph name="CLOSE_SMALL" :size="18" />
          </button>
        </div>
        <div class="dialog-content">
          <div class="download-selection-text">
            请选择要下载的资源范围：原画仅保留视频中的原画条目。
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button outline-action dialog-action"
            type="button"
            :style="{ width: actionWidth('下载全部') }"
            @click="beginScopeDownload('all')"
          >
            下载全部
          </button>
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('下载原画') }"
            @click="beginScopeDownload('original')"
          >
            下载原画
          </button>
        </div>
      </template>

      <template v-else-if="state.dialog === 'download-progress'">
        <div class="dialog-title-row">
          <div class="dialog-title" style="font-size: 20px">下载进度</div>
          <button
            class="dialog-close-button"
            type="button"
            aria-label="取消下载"
            @click="cancelDownload"
          >
            <IconGlyph name="CLOSE_SMALL" :size="18" />
          </button>
        </div>
        <div v-if="download" class="dialog-content download-progress-content">
          <div class="progress-section">
            <div class="progress-heading">
              <span>总体进度:</span>
              <span class="progress-success">
                {{ download.completed }}/{{ download.total }} 已完成
              </span>
            </div>
            <div class="progress-track">
              <div class="progress-fill" :style="{ width: `${download.overall * 100}%` }" />
            </div>
            <div class="progress-numbers">
              <span>{{ (download.overall * 100).toFixed(1) }}%</span>
              <span>
                {{ formatSize(download.downloaded) }} /
                {{
                  download.totalSize > 0
                    ? formatSize(download.totalSize)
                    : "计算中..."
                }}
              </span>
            </div>
          </div>
          <div class="progress-section">
            <div class="progress-current-label">
              当前视频 ({{ Math.min(download.current + 1, download.total) }}/{{ download.total }}):
            </div>
            <div class="progress-track">
              <div
                class="progress-fill"
                :style="{ width: `${download.currentProgress * 100}%` }"
              />
            </div>
            <div class="progress-numbers">
              <span>{{ (download.currentProgress * 100).toFixed(1) }}%</span>
              <span>
                {{ formatSize(download.currentSize) }} /
                {{
                  formatSize(
                    Math.max(download.currentTotal, download.currentSize),
                  )
                }}
              </span>
            </div>
          </div>
          <div class="download-status" :class="{ complete: allCompleted }">
            {{
              allCompleted
                ? "所有下载完成！正在关闭进度窗口..."
                : "正在顺序下载..."
            }}
          </div>
        </div>
      </template>

      <template v-else-if="state.dialog === 'download-saved'">
        <div class="dialog-result-title">
          <IconGlyph name="CHECK_CIRCLE" :size="24" mono color="#05c001" />
          <span>下载完成</span>
        </div>
        <div class="dialog-content download-saved-content">
          <div class="download-saved-summary">
            {{ state.downloadSaved.count }} 个资源已保存到系统相册{{
              state.downloadSaved.failed
                ? `，${state.downloadSaved.failed} 个下载失败`
                : ""
            }}
          </div>
          <div
            v-if="state.downloadSaved.skipped > 0"
            class="dialog-secondary-text"
          >
            已跳过 {{ state.downloadSaved.skipped }} 个 MP3
            文件（相册不支持音频）
          </div>
          <div
            v-if="state.downloadSaved.timedOut > 0"
            class="dialog-secondary-text"
          >
            已跳过 {{ state.downloadSaved.timedOut }} 个超时链接（超过 10
            秒无响应）
          </div>
        </div>
        <div class="dialog-actions">
          <button
            class="action-button primary-action dialog-action"
            type="button"
            :style="{ width: actionWidth('知道了') }"
            @click="closeDialog"
          >
            知道了
          </button>
        </div>
      </template>
    </div>
  </div>
</template>
