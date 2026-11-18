package dev.gpui.mobile;

import android.animation.ValueAnimator;
import android.app.NativeActivity;
import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.ClipboardManager;
import android.content.ClipData;
import android.content.Intent;
import android.content.ComponentName;
import android.content.ContentValues;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.content.res.ColorStateList;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.ImageDecoder;
import android.graphics.Insets;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Rect;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.media.ExifInterface;
import android.net.Uri;
import android.provider.MediaStore;
import android.os.Build;
import android.os.Bundle;
import android.os.Looper;
import android.os.SystemClock;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.text.Editable;
import android.text.InputType;
import android.text.Layout;
import android.text.Selection;
import android.text.Spannable;
import android.text.Spanned;
import android.text.SpannableStringBuilder;
import android.text.TextPaint;
import android.text.TextWatcher;
import android.text.style.LineHeightSpan;
import android.text.method.ArrowKeyMovementMethod;
import android.util.Base64;
import android.util.Log;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.ActionMode;
import android.view.KeyEvent;
import android.view.Menu;
import android.view.MenuInflater;
import android.view.MenuItem;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.view.WindowInsetsAnimation;
import android.view.WindowInsetsController;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.view.textclassifier.TextClassifier;
import android.view.animation.DecelerateInterpolator;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import android.widget.FrameLayout;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.PopupMenu;
import android.widget.PopupWindow;
import android.widget.TextView;
import android.graphics.Typeface;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.security.KeyStore;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import androidx.core.splashscreen.SplashScreen;
import androidx.activity.OnBackPressedCallback;
import androidx.activity.OnBackPressedDispatcher;

/**
 * NativeActivity host used by GPUI Android applications.
 *
 * <p>The small editor view supplies the InputConnection that NativeActivity
 * itself lacks, enabling full commit/composition callbacks for software IMEs.
 * Credential helpers keep encrypted payloads in app-private preferences and
 * the encryption key in AndroidKeyStore.</p>
 */
public class GpuiActivity extends NativeActivity {
    private static final int LIGHT_SELECTION_HIGHLIGHT_COLOR = 0x553B82F6;
    private static final int DARK_SELECTION_HIGHLIGHT_COLOR = 0x9960A5FA;
    private static final int LIGHT_SELECTION_HANDLE_COLOR = 0xFF3B82F6;
    private static final int DARK_SELECTION_HANDLE_COLOR = 0xFF60A5FA;
    private void applyExactLineHeight() {
        applyExactLineHeight(true);
    }

    private void applyExactLineHeight(boolean forceLayout) {
        if (textMenuEditor == null || editorCornerRadius < 0.0f) {
            return;
        }
        Editable editable = textMenuEditor.getText();
        if (!(editable instanceof Spannable)) {
            return;
        }
        Spannable spannable = (Spannable) editable;
        int lineHeightPx = Math.round(editorTargetLineHeightPx);
        if (editable.length() == 0) {
            spannable.removeSpan(exactLineHeightSpan);
            return;
        }
        boolean lineHeightChanged = exactLineHeightSpan.setLineHeightPx(lineHeightPx);
        int spanStart = spannable.getSpanStart(exactLineHeightSpan);
        int spanEnd = spannable.getSpanEnd(exactLineHeightSpan);
        // Text edits resize an attached span automatically. Re-applying it
        // over the whole document after every character forces TextView to
        // rebuild the line layout and makes held-backspace visibly slower as
        // the draft grows. Only repair missing coverage or changed metrics.
        boolean coverageChanged = spanStart != 0 || spanEnd < editable.length();
        if (!lineHeightChanged && !coverageChanged) {
            return;
        }
        spannable.setSpan(
                exactLineHeightSpan,
                0,
                editable.length(),
                Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        if (forceLayout && lineHeightChanged) {
            textMenuEditor.requestLayout();
            textMenuEditor.invalidate();
        }
    }

    private CharSequence textWithExactLineHeight(CharSequence source) {
        if (editorTargetLineHeightPx <= 0 || source.length() == 0) {
            return source;
        }
        SpannableStringBuilder styled = new SpannableStringBuilder(source);
        exactLineHeightSpan.setLineHeightPx(Math.round(editorTargetLineHeightPx));
        styled.setSpan(
                exactLineHeightSpan,
                0,
                styled.length(),
                Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        return styled;
    }

    public boolean saveMediaToGallery(String path) {
        File file = new File(path);
        if (!file.isFile()) {
            return false;
        }
        String name = file.getName();
        int dot = name.lastIndexOf('.');
        String extension = dot >= 0 ? name.substring(dot + 1).toLowerCase() : "";
        boolean video;
        String mime;
        switch (extension) {
            case "mp4":
            case "m4v":
            case "mov":
                video = true;
                mime = "video/" + extension;
                break;
            case "jpeg":
            case "jpg":
                video = false;
                mime = "image/jpeg";
                break;
            case "png":
                video = false;
                mime = "image/png";
                break;
            case "webp":
                video = false;
                mime = "image/webp";
                break;
            case "gif":
                video = false;
                mime = "image/gif";
                break;
            case "heic":
                video = false;
                mime = "image/heic";
                break;
            default:
                video = false;
                mime = "application/octet-stream";
        }

        Context context = getApplicationContext();
        ContentValues values = new ContentValues();
        values.put(MediaStore.MediaColumns.DISPLAY_NAME, name);
        values.put(MediaStore.MediaColumns.MIME_TYPE, mime);
        if (Build.VERSION.SDK_INT >= 29) {
            values.put(
                MediaStore.MediaColumns.RELATIVE_PATH,
                video ? "Movies/Reels" : "Pictures/Reels");
            values.put(MediaStore.MediaColumns.IS_PENDING, 1);
        }

        Uri uri = context.getContentResolver().insert(
            video
                ? MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
                : MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY),
            values);
        if (uri == null) {
            return false;
        }

        try {
            try (java.io.InputStream input = new java.io.FileInputStream(file);
                 java.io.OutputStream output = context.getContentResolver().openOutputStream(uri)) {
                if (output == null) {
                    return false;
                }
                byte[] buffer = new byte[64 * 1024];
                int length;
                while ((length = input.read(buffer)) > 0) {
                    output.write(buffer, 0, length);
                }
            }
            if (Build.VERSION.SDK_INT >= 29) {
                values.clear();
                values.put(MediaStore.MediaColumns.IS_PENDING, 0);
                context.getContentResolver().update(uri, values, null, null);
            }
            return true;
        } catch (IOException error) {
            context.getContentResolver().delete(uri, null, null);
            return false;
        }
    }

    /**
     * 把系统能解码的图片（HEIC/HEIF/AVIF 等）转成 JPEG。
     * maxDimension 小于等于 0 时保持原始分辨率。
     */
    public boolean convertImageToJpeg(
            String sourcePath, String targetPath, int maxDimension, int quality) {
        File source = new File(sourcePath);
        if (!source.isFile()) {
            return false;
        }
        int clampedQuality = Math.max(1, Math.min(quality, 100));
        Bitmap bitmap = decodeImage(source, maxDimension);
        if (bitmap == null) {
            return false;
        }
        try (FileOutputStream output = new FileOutputStream(targetPath)) {
            return bitmap.compress(Bitmap.CompressFormat.JPEG, clampedQuality, output);
        } catch (IOException error) {
            return false;
        } finally {
            bitmap.recycle();
        }
    }

    private static Bitmap decodeImage(File source, int maxDimension) {
        if (Build.VERSION.SDK_INT >= 28) {
            try {
                ImageDecoder.Source decoderSource = ImageDecoder.createSource(source);
                return ImageDecoder.decodeBitmap(decoderSource, (decoder, info, src) -> {
                    decoder.setAllocator(ImageDecoder.ALLOCATOR_SOFTWARE);
                    decoder.setTargetSampleSize(sampleSize(
                            info.getSize().getWidth(),
                            info.getSize().getHeight(),
                            maxDimension));
                });
            } catch (Throwable ignored) {
                // ImageDecoder 失败时回退到 BitmapFactory。
            }
        }
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeFile(source.getAbsolutePath(), bounds);
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight, maxDimension);
        Bitmap bitmap = BitmapFactory.decodeFile(source.getAbsolutePath(), options);
        if (bitmap == null) {
            return null;
        }
        return applyOrientation(bitmap, readOrientation(source.getAbsolutePath()));
    }

    private static int sampleSize(int width, int height, int maxDimension) {
        int longest = Math.max(width, height);
        int sample = 1;
        if (maxDimension > 0) {
            while (longest / (sample * 2) >= maxDimension) {
                sample *= 2;
            }
        }
        return sample;
    }

    private static int readOrientation(String path) {
        try {
            ExifInterface exif = new ExifInterface(path);
            return exif.getAttributeInt(
                    ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL);
        } catch (IOException | RuntimeException error) {
            return ExifInterface.ORIENTATION_NORMAL;
        }
    }

    private static Bitmap applyOrientation(Bitmap bitmap, int orientation) {
        Matrix matrix = new Matrix();
        switch (orientation) {
            case ExifInterface.ORIENTATION_ROTATE_90:
                matrix.postRotate(90);
                break;
            case ExifInterface.ORIENTATION_ROTATE_180:
                matrix.postRotate(180);
                break;
            case ExifInterface.ORIENTATION_ROTATE_270:
                matrix.postRotate(270);
                break;
            case ExifInterface.ORIENTATION_FLIP_HORIZONTAL:
                matrix.postScale(-1, 1);
                break;
            case ExifInterface.ORIENTATION_FLIP_VERTICAL:
                matrix.postScale(1, -1);
                break;
            default:
                return bitmap;
        }
        Bitmap rotated = Bitmap.createBitmap(
                bitmap, 0, 0, bitmap.getWidth(), bitmap.getHeight(), matrix, true);
        if (rotated != bitmap) {
            bitmap.recycle();
        }
        return rotated;
    }

    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String CREDENTIAL_PREFS = "gpui_secure_credentials";
    private GpuiInputView inputView;
    private final AtomicBoolean accessibilityChangeScheduled = new AtomicBoolean();
    private final OnBackPressedDispatcher onBackPressedDispatcher =
            new OnBackPressedDispatcher();
    private OnBackPressedCallback gpuiBackCallback;

    private static native boolean nativeIsInitialized();
    private static native boolean nativeIsFirstFramePresented();
    private static native void nativeResetFirstFramePresented();
    private static native void nativeOnBackPressed();
    private static native void nativeOnInsetsChanged(
            int left,
            int top,
            int right,
            int bottom,
            int imeLeft,
            int imeTop,
            int imeRight,
            int imeBottom);
    private static native void nativeOnDeepLink(String url);
    private static native void nativeCommitText(String text);
    private static native void nativeCopySelection();
    private static native void nativeCutSelection();
    private static native void nativeMirrorTextChanged(
            String text, int selStart, int selEnd, long session);
    private static native void nativeMirrorSelectionChanged(int selStart, int selEnd);
    private static native void nativeMirrorScrollChanged(float scrollTop);
    private static native void nativeMirrorLineRangesChanged(String text, String ranges);
    private static native void nativeOnNativeEditorHidden();
    private static native void nativeOnNativeEditorFocusChanged(boolean focused);
    private static native void nativeOnNativeEditorWindowFocusChanged(boolean ownsFocus);
    private static native void nativeOnNativeEditorImeAnimationEnd();
    private static native void nativeOnNativeEditorImePreviewVisible(boolean visible);
    private static native void nativeOnNativeCookieSave(String text);
    private static native void nativeOnNativeCookieCancel();
    private static native void nativeOnResumeCoverFadeStart();
    private static native void nativeSetComposingText(String text);
    private static native void nativeFinishComposingText();
    private static native void nativeDeleteSurroundingText(int before, int after);
    private static native String nativeAccessibilitySnapshot();
    private static native boolean nativeAccessibilityAction(long nodeId, int action);

    /**
     * GPUI colors are packed as 0xRRGGBBAA, while Android text and drawable
     * colors use 0xAARRGGBB. Without this conversion an opaque GPUI color
     * becomes an almost transparent Android color, which can make selected
     * text disappear against the editor background.
     */
    private static int gpuiColorToArgb(int color) {
        int red = (color >>> 24) & 0xFF;
        int green = (color >>> 16) & 0xFF;
        int blue = (color >>> 8) & 0xFF;
        int alpha = color & 0xFF;
        return (alpha << 24) | (red << 16) | (green << 8) | blue;
    }

    /**
     * Resolve splash/launch resources with the app's saved theme instead of
     * the system's, so a light app preference shows a white launch cover even
     * on a dark system (and vice versa). Runs before the splash theme binds.
     */
    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(applyAppThemeConfiguration(base));
    }

    private Context applyAppThemeConfiguration(Context base) {
        File settingsFile = new File(base.getFilesDir(), "config/settings.json");
        try {
            String text = new String(
                    java.nio.file.Files.readAllBytes(settingsFile.toPath()),
                    java.nio.charset.StandardCharsets.UTF_8);
            String mode = new JSONObject(text).optString("theme_mode", "system");
            int nightMode;
            if ("light".equals(mode)) {
                nightMode = Configuration.UI_MODE_NIGHT_NO;
            } else if ("dark".equals(mode)) {
                nightMode = Configuration.UI_MODE_NIGHT_YES;
            } else {
                return base;
            }
            Configuration config = new Configuration(base.getResources().getConfiguration());
            config.uiMode = (config.uiMode & ~Configuration.UI_MODE_NIGHT_MASK) | nightMode;
            return base.createConfigurationContext(config);
        } catch (Exception error) {
            return base;
        }
    }

    /**
     * Apply the App theme's system-bar color and icon contrast immediately.
     *
     * <p>After an Activity relaunch Android can keep the launcher's light/dark
     * status-bar appearance until the first GPUI frame runs. That leaves the
     * clock and system icons in the wrong color while the resume cover is
     * still showing. `attachBaseContext` already resolves the saved App theme
     * into this Activity's configuration, so mirror that choice here before
     * the first frame.</p>
     */
    private void applyAppSystemBarAppearance() {
        int nightMode = getResources().getConfiguration().uiMode
                & Configuration.UI_MODE_NIGHT_MASK;
        boolean dark = nightMode == Configuration.UI_MODE_NIGHT_YES;
        int background = dark ? 0xFF141414 : 0xFFFFFFFF;

        getWindow().setStatusBarColor(background);
        getWindow().setNavigationBarColor(background);

        View decor = getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                int mask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                        | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
                controller.setSystemBarsAppearance(dark ? 0 : mask, mask);
            }
        } else {
            int lightBars = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                    | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
            int flags = decor.getSystemUiVisibility();
            flags = dark ? flags & ~lightBars : flags | lightBars;
            decor.setSystemUiVisibility(flags);
        }
    }

    /**
     * Keep the launcher entry on the icon that matches the system theme.
     *
     * <p>The app may pin itself to a light or dark theme, so this reads the
     * system configuration instead of the Activity's uiMode override. The
     * launcher aliases target an invisible proxy Activity; changing their
     * enabled state therefore does not finish this Activity or its task.</p>
     */
    private void applySystemLauncherIcon() {
        boolean systemDark = isSystemDarkMode();
        String packageName = getPackageName();
        ComponentName lightAlias = new ComponentName(
                packageName, packageName + ".LauncherLight");
        ComponentName darkAlias = new ComponentName(
                packageName, packageName + ".LauncherDark");
        PackageManager packageManager = getPackageManager();

        int lightState = packageManager.getComponentEnabledSetting(lightAlias);
        int darkState = packageManager.getComponentEnabledSetting(darkAlias);
        boolean lightEnabled = lightState == PackageManager.COMPONENT_ENABLED_STATE_ENABLED
                || lightState == PackageManager.COMPONENT_ENABLED_STATE_DEFAULT;
        boolean darkEnabled = darkState == PackageManager.COMPONENT_ENABLED_STATE_ENABLED;
        boolean desiredEnabled = systemDark ? darkEnabled : lightEnabled;
        boolean otherDisabled = systemDark ? !lightEnabled : !darkEnabled;
        if (desiredEnabled && otherDisabled) {
            return;
        }

        ComponentName desired = systemDark ? darkAlias : lightAlias;
        ComponentName other = systemDark ? lightAlias : darkAlias;
        try {
            if (!desiredEnabled) {
                packageManager.setComponentEnabledSetting(
                        desired,
                        PackageManager.COMPONENT_ENABLED_STATE_ENABLED,
                        PackageManager.DONT_KILL_APP);
            }
            if (!otherDisabled) {
                packageManager.setComponentEnabledSetting(
                        other,
                        PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
                        PackageManager.DONT_KILL_APP);
            }
        } catch (RuntimeException error) {
            android.util.Log.w(
                    "GpuiActivity",
                    "Unable to switch system launcher icon",
                    error);
        }
    }

    private boolean isSystemDarkMode() {
        int systemNightMode = android.content.res.Resources.getSystem()
                .getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
        return systemNightMode == Configuration.UI_MODE_NIGHT_YES;
    }

    /**
     * Run editor-window updates on the Android UI thread and wait briefly for
     * completion. GPUI paints and presents on the native thread; blocking here
     * until WindowManager applies the change keeps the EditText window and the
     * GPUI frame on the same visible frame, so dialogs and the native field
     * never show one frame apart. The timeout prevents an ANR if the UI thread
     * is temporarily busy with a lifecycle transition.
     */
    private void runOnUiThreadBlocking(Runnable action) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            action.run();
            return;
        }
        CountDownLatch latch = new CountDownLatch(1);
        runOnUiThread(() -> {
            try {
                action.run();
            } finally {
                latch.countDown();
            }
        });
        try {
            if (!latch.await(100, TimeUnit.MILLISECONDS)) {
                android.util.Log.w("GpuiActivity", "UI-thread editor sync timed out");
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
        }
    }

    @Override
    protected void onCreate(Bundle state) {
        ensureNativeLibraryVisibleToJvm();
        nativeResetFirstFramePresented();
        SplashScreen splashScreen = SplashScreen.installSplashScreen(this);
        splashScreen.setKeepOnScreenCondition(
                () -> !nativeIsFirstFramePresented());
        super.onCreate(state);
        gpuiBackCallback = new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                if (nativeCookiePanelActive) {
                    handleNativeCookieBack();
                    return;
                }
                if (textMenuEditor != null
                        && textMenuEditor.getVisibility() == View.VISIBLE
                        && textMenuEditor.hasFocus()
                        && (imeVisible || imeInsetsAnimating)) {
                    if (imeVisible || imeInsetsAnimating) {
                        hideNativeEditorBeforeImePreview();
                    }
                    blurTextMenuEditor();
                    InputMethodManager manager =
                            (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
                    if (manager != null) {
                        manager.hideSoftInputFromWindow(inputView.getWindowToken(), 0);
                    }
                    return;
                }
                nativeOnBackPressed();
            }
        };
        onBackPressedDispatcher.addCallback(gpuiBackCallback);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            onBackPressedDispatcher.setOnBackInvokedDispatcher(getOnBackInvokedDispatcher());
        }
        textMenuEditor = new MirrorEditText(this);
        // setHint() before the first addView() calls TextView.checkForRelayout(),
        // which dereferences LayoutParams. Give the detached view a default set
        // so the first style pass cannot abort before the editor is attached.
        textMenuEditor.setLayoutParams(new WindowManager.LayoutParams());
        textMenuEditor.setBackground(null);
        textMenuEditor.setTextColor(ColorStateList.valueOf(0xFFFFFFFF));
        textMenuEditor.setHintTextColor(0xFF808080);
        textMenuEditor.setIncludeFontPadding(false);
        textMenuEditor.setLineSpacing(0.0f, 1.0f);
        textMenuEditor.setImeOptions(
                EditorInfo.IME_ACTION_NONE
                        | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                        | EditorInfo.IME_FLAG_NO_FULLSCREEN);
        textMenuEditor.setVerticalScrollBarEnabled(false);
        textMenuEditor.setHorizontalScrollBarEnabled(false);
        textMenuEditor.setTypeface(Typeface.SANS_SERIF);
        textMenuEditor.setLetterSpacing(0.0f);
        textMenuEditor.setBreakStrategy(Layout.BREAK_STRATEGY_SIMPLE);
        textMenuEditor.setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE);
        applyNativeSelectionColors(false);
        textMenuEditor.setCursorVisible(true);
        textMenuEditor.setTextIsSelectable(true);
        textMenuEditor.setMovementMethod(ArrowKeyMovementMethod.getInstance());
        textMenuEditor.setGravity(Gravity.TOP | Gravity.START);
        textMenuEditor.setPadding(0, 0, 0, 0);
        textMenuEditor.setClickable(true);
        textMenuEditor.setFocusable(false);
        textMenuEditor.setFocusableInTouchMode(false);
        textMenuEditor.setTextIsSelectable(false);
        textMenuEditor.setCursorVisible(false);
        textMenuEditor.setVisibility(View.GONE);
        inputView = new GpuiInputView(this, textMenuEditor);
        inputView.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT);
        params.gravity = Gravity.BOTTOM | Gravity.END;
        addContentView(inputView, params);
        textMenuEditor.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {}

            @Override
            public void afterTextChanged(Editable s) {
                if (textMenuEditor == null
                        || textMenuEditor.getVisibility() != View.VISIBLE) {
                    return;
                }
                if (mirrorSyncing) {
                    nativeEditorSnapshot = s.toString();
                    return;
                }
                textMenuEditor.scheduleMirrorTextNotify();
                textMenuEditor.flushLineRangesNotify();
            }
        });
        View.OnApplyWindowInsetsListener insetsListener = (view, insets) -> {
            applyWindowInsets(insets, false);
            return insets;
        };
        inputView.setOnApplyWindowInsetsListener(insetsListener);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // The shadow input view is made INVISIBLE while the real editor
            // owns focus, and Android stops delivering IME animation
            // callbacks to an invisible subtree. Keep the callback on the
            // always-visible decor view instead; applying each progress
            // sample through `inputView` still uses the same inset pipeline.
            getWindow().getDecorView().setWindowInsetsAnimationCallback(
                    new WindowInsetsAnimation.Callback(
                            WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                        @Override
                        public WindowInsetsAnimation.Bounds onStart(
                                WindowInsetsAnimation animation,
                                WindowInsetsAnimation.Bounds bounds) {
                            super.onStart(animation, bounds);
                            if ((animation.getTypeMask() & WindowInsets.Type.ime()) != 0) {
                                imeInsetsAnimating = true;
                                android.util.Log.d(
                                        "GPUIInsetsAnim",
                                        "start preview=" + nativeEditorImePreview
                                                + " attached=" + nativeEditorAttached);
                                if (nativeEditorImePreview
                                        && nativeEditorAttached
                                        && textMenuEditor != null) {
                                    nativeEditorImePreviewAnimating = true;
                                    nativeEditorImePreviewShowPending = false;
                                    nativeOnNativeEditorImePreviewVisible(true);
                                    setNativeEditorContentAlpha(0.0f);
                                }
                            }
                            return bounds;
                        }

                        @Override
                        public WindowInsets onProgress(
                                WindowInsets insets,
                                java.util.List<WindowInsetsAnimation> runningAnimations) {
                            if (imeInsetsAnimating) {
                                applyWindowInsets(insets, true);
                            }
                            return insets;
                        }

                        @Override
                        public void onEnd(WindowInsetsAnimation animation) {
                            super.onEnd(animation);
                            if ((animation.getTypeMask() & WindowInsets.Type.ime()) == 0) {
                                return;
                            }
                            android.util.Log.d(
                                    "GPUIInsetsAnim",
                                    "end preview=" + nativeEditorImePreview
                                            + " active=" + nativeEditorImePreviewAnimating);
                            imeInsetsAnimating = false;
                            WindowInsets finalInsets = inputView.getRootWindowInsets();
                            // Some ROMs still report the pre-animation
                            // snapshot here; applying it would drag the card
                            // and editor back to the old position.
                            if (finalInsets != null
                                    && imeBottomOf(finalInsets) != imeBottomPixels) {
                                applyWindowInsets(finalInsets, false);
                            }
                            if (nativeEditorImePreview && nativeEditorImePreviewAnimating) {
                                nativeEditorImePreviewShowPending = true;
                                nativeOnNativeEditorImeAnimationEnd();
                            }
                        }
                    });
        }
        inputView.requestApplyInsets();
        applyAppSystemBarAppearance();
        dispatchDeepLink(getIntent());
    }

    /**
     * Associate NativeActivity's library with this Java class loader.
     *
     * <p>NativeActivity loads {@code android.app.lib_name} through its native
     * glue. That is enough for {@code ANativeActivity_onCreate}, but Android's
     * Java JNI resolver does not associate that dlopen handle with this class
     * loader. Explicitly loading the same library is idempotent and makes the
     * input, accessibility, deep-link, and credential JNI entry points
     * resolvable before Android invokes them.</p>
     */
    private void ensureNativeLibraryVisibleToJvm() {
        try {
            ActivityInfo info = getPackageManager().getActivityInfo(
                    new ComponentName(this, getClass()), PackageManager.GET_META_DATA);
            String library = info.metaData == null
                    ? null
                    : info.metaData.getString("android.app.lib_name");
            if (library == null || library.isEmpty()) {
                throw new IllegalStateException(
                        "android.app.lib_name is required for GPUI NativeActivity hosts");
            }
            System.loadLibrary(library);
        } catch (PackageManager.NameNotFoundException error) {
            throw new IllegalStateException("Unable to resolve the GPUI activity metadata", error);
        }
    }

    /**
     * Keep the system selection-handle geometry while applying the app's
     * primary blue. The platform theme provides the correct handle shape on
     * every vendor ROM, including pads, tear drops and accessibility sizes.
     */
    private void applyNativeSelectionColors(boolean dark) {
        if (textMenuEditor == null) {
            return;
        }
        int highlightColor = dark
                ? DARK_SELECTION_HIGHLIGHT_COLOR
                : LIGHT_SELECTION_HIGHLIGHT_COLOR;
        textMenuEditor.setHighlightColor(highlightColor);
        if (textMenuEditor == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return;
        }
        int handleColor = dark
                ? DARK_SELECTION_HANDLE_COLOR
                : LIGHT_SELECTION_HANDLE_COLOR;
        Drawable center = textMenuEditor.getTextSelectHandle();
        if (center != null) {
            textMenuEditor.setTextSelectHandle(
                    tintSelectionHandle(center.mutate(), handleColor));
        }

        if (textMenuEditor.selectHandleLeftHidden) {
            if (textMenuEditor.selectHandleLeftDrawable != null) {
                textMenuEditor.selectHandleLeftDrawable.setTint(handleColor);
            }
        } else {
            Drawable left = textMenuEditor.getTextSelectHandleLeft();
            if (left != null) {
                textMenuEditor.setTextSelectHandleLeft(
                        tintSelectionHandle(left.mutate(), handleColor));
            }
        }

        if (textMenuEditor.selectHandleRightHidden) {
            if (textMenuEditor.selectHandleRightDrawable != null) {
                textMenuEditor.selectHandleRightDrawable.setTint(handleColor);
            }
        } else {
            Drawable right = textMenuEditor.getTextSelectHandleRight();
            if (right != null) {
                textMenuEditor.setTextSelectHandleRight(
                        tintSelectionHandle(right.mutate(), handleColor));
            }
        }
    }

    private static Drawable tintSelectionHandle(Drawable handle, int color) {
        handle.setTint(color);
        return handle;
    }

    private static boolean isDarkBackgroundColor(int argb) {
        int red = (argb >>> 16) & 0xFF;
        int green = (argb >>> 8) & 0xFF;
        int blue = argb & 0xFF;
        int luminance = (red * 299 + green * 587 + blue * 114) / 1000;
        return luminance < 128;
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        dispatchDeepLink(intent);
    }

    @Override
    public void onBackPressed() {
        onBackPressedDispatcher.onBackPressed();
    }

    /** Finish the task and terminate the app process. */
    public void gpuiExitApp() {
        runOnUiThreadBlocking(() -> {
            finishAndRemoveTask();
            android.os.Process.killProcess(android.os.Process.myPid());
            System.exit(0);
        });
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        // The native editor lives in an application-panel window. When that
        // window takes focus the Activity itself reports hasFocus=false.
        // Tell the Rust event bridge not to treat that as the app entering
        // the background, otherwise GPUI stops rendering while the user is
        // actively typing and the mirrored text never reaches the UI state.
        nativeOnNativeEditorWindowFocusChanged(
                !hasFocus
                        && ((nativeEditorAttached && nativeEditorFocusableMode)
                                || nativeCookiePanelAttached));
        android.util.Log.d(
                "GPUI_NATIVE_INPUT",
                "activityWindowFocus=" + hasFocus
                        + " attached=" + nativeEditorAttached
                        + " focusable=" + nativeEditorFocusableMode
                        + " editorFocus=" + (textMenuEditor != null && textMenuEditor.hasFocus()));
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            applyAppSystemBarAppearance();
        }
    }

    @Override
    protected void onPause() {
        // Do not blur the panel editor based on Activity pause/insets state.
        // Some IMEs briefly pause the host or report zero IME insets while
        // opening their input UI; clearing focus here immediately dismisses
        // the keyboard we just requested. Explicit Back/blur actions end the
        // editor session instead.
        super.onPause();
        // Record the last rendered frame while the window surface is still
        // valid; it becomes the resume cover's background.
        captureResumeCover();
    }

    @Override
    protected void onStop() {
        super.onStop();
        // The activity is no longer visible (freeze/background). Remove the
        // separate editor window so it cannot hover above the frozen surface
        // or reappear before GPUI presents its first resumed frame. A later
        // tap triggers a fresh, geometry-synced handoff.
        editorWasAttachedAtStop = nativeEditorAttached;
        resumeEditorText = textMenuEditor != null
                ? textMenuEditor.getText().toString() : null;
        if (textMenuEditor != null && nativeEditorAttached) {
            hideTextMenuEditor(true);
        }
        // The Cookie panel is its own window; remove it so it cannot hover
        // above the frozen surface or another app. Rust keeps the dialog
        // state, and onResume re-attaches it when the dialog is still open.
        detachNativeCookiePanel();
        resumeCoverNeeded = true;
    }

    @Override
    protected void onStart() {
        super.onStart();
        applyAppSystemBarAppearance();
        applySystemLauncherIcon();
        // Attach the resume cover as early as possible. The task-open
        // animation can start before onResume, and while the SurfaceView has
        // no first frame the transparent task window would let the launcher
        // show straight through the app. With the cover already attached the
        // first visible content is the opaque theme background instead.
        if (resumeCoverNeeded) {
            showResumeCover();
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        applySystemLauncherIcon();
        if (resumeCoverNeeded) {
            showResumeCover();
        }
        if (nativeCookiePanelActive && !nativeCookiePanelAttached) {
            // Re-create the window without raising the keyboard; the user was
            // editing this dialog before the app went to the background.
            attachNativeCookiePanel(false);
        }
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        applySystemLauncherIcon();
        // Window metrics changed orientation; the cookie panel is positioned
        // against the display bounds, so re-centre it even if the inset
        // callback does not fire again. Rust only sends the card width when
        // the dialog is shown, so the width has to be re-derived here.
        if (nativeCookiePanelActive) {
            recomputeNativeCookiePanelWidth();
        }
        if (nativeCookiePanelAttached) {
            layoutNativeCookiePanel();
            nativeCookiePanel.requestApplyInsets();
        }
    }

    /**
     * Full-screen cover shown between the activity becoming visible again and
     * GPUI submitting its first resumed frame. Without it the frozen app
     * resumes onto a black surface and the home input appears to float over
     * nothing. The cover is painted with the launch background (which follows
     * the app's theme via the attachBaseContext uiMode override) and mirrors
     * the native editor's cached geometry so the input box appears to stay in
     * place. The editor window sits above this panel, and GPUI renders below
     * it, so neither is occluded.
     */
    private void showResumeCover() {
        if (resumeCoverAttached) {
            // If onStart had to fall back to a plain application window
            // (decor token not ready yet), upgrade to the full-screen panel
            // as soon as the token exists, so the cover also covers the
            // status bar / notch strip instead of leaving a background band.
            if (!resumeCoverUsedPanel
                    && resumeCoverView != null
                    && getWindow().getDecorView().getWindowToken() != null) {
                upgradeResumeCoverToPanel();
            }
            if (resumeCoverView != null) {
                resumeCoverView.animate().cancel();
                resumeCoverView.setAlpha(1f);
            }
            return;
        }
        if (resumeCoverView == null) {
            resumeCoverView = new android.widget.FrameLayout(this);
        }
        // The calibrated bitmap may use negative margins to reach the status
        // bar strip; never clip it to the (possibly inset) window frame.
        resumeCoverView.setClipChildren(false);
        // Show the last rendered frame (captured at onPause) instead of a
        // plain color: it is pixel-identical to what the task snapshot shows,
        // so the snapshot-to-cover transition is invisible, and the cover is
        // replaced 1:1 by the live GPUI frame at reveal time. The editor
        // panel is a separate window and is not part of the capture; the
        // live EditText is re-attached below to supply its content.
        resumeCoverView.removeAllViews();
        android.util.TypedValue windowBackground = new android.util.TypedValue();
        getTheme().resolveAttribute(android.R.attr.windowBackground, windowBackground, true);
        resumeCoverView.setBackgroundColor(windowBackground.data);
        if (resumeCoverBitmap != null && !resumeCoverBitmap.isRecycled()) {
            android.widget.ImageView lastFrame = new android.widget.ImageView(this);
            lastFrame.setImageBitmap(resumeCoverBitmap);
            lastFrame.setScaleType(android.widget.ImageView.ScaleType.FIT_XY);
            // The capture is the full window surface in screen coordinates.
            // Panel windows may lay out relative to a frame that starts below
            // the status bar, so the margins are re-calibrated after layout
            // (see below) against the measured on-screen origin.
            android.widget.FrameLayout.LayoutParams frameParams =
                    new android.widget.FrameLayout.LayoutParams(
                            resumeCoverBitmap.getWidth(),
                            resumeCoverBitmap.getHeight());
            frameParams.gravity = Gravity.TOP | Gravity.START;
            frameParams.leftMargin = resumeCoverOriginX;
            frameParams.topMargin = resumeCoverOriginY;
            resumeCoverView.addView(lastFrame, frameParams);
        }
        WindowManager wm = getWindow().getWindowManager();
        if (wm == null) {
            return;
        }
        if (resumeCoverParams == null) {
            resumeCoverParams = new WindowManager.LayoutParams(
                    WindowManager.LayoutParams.MATCH_PARENT,
                    WindowManager.LayoutParams.MATCH_PARENT,
                    WindowManager.LayoutParams.TYPE_APPLICATION,
                    WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
                            | WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                            | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                            | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                            | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
                    android.graphics.PixelFormat.TRANSLUCENT);
            resumeCoverParams.gravity = Gravity.TOP | Gravity.START;
            resumeCoverParams.setTitle("ReelsResumeCover");
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                resumeCoverParams.setFitInsetsTypes(0);
            }
            // Cover the notch/cutout strip too, otherwise it flashes during
            // the covered gap.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                resumeCoverParams.layoutInDisplayCutoutMode =
                        WindowManager.LayoutParams
                                .LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            }
        }
        try {
            // Prefer an activity panel: its frame is the decor view's
            // edge-to-edge frame, so the cover reaches the status bar /
            // cutout strip and the captured frame aligns 1:1 without any
            // cropping. Fall back to a plain application window if the token
            // is not usable yet (upgraded later by showResumeCover).
            try {
                resumeCoverParams.type =
                        WindowManager.LayoutParams.TYPE_APPLICATION_PANEL;
                resumeCoverParams.token = getWindow().getDecorView().getWindowToken();
                wm.addView(resumeCoverView, resumeCoverParams);
                resumeCoverUsedPanel = true;
            } catch (Exception panelError) {
                resumeCoverParams.type =
                        WindowManager.LayoutParams.TYPE_APPLICATION;
                resumeCoverParams.token = null;
                wm.addView(resumeCoverView, resumeCoverParams);
                resumeCoverUsedPanel = false;
            }
            resumeCoverAttached = true;
            scheduleResumeCoverCalibration();
            // Attach the display-only editor AFTER the cover: windows added
            // later stack on top, and the editor must sit above the cover so
            // its text/placeholder are visible during the covered gap.
            if (resumeCoverBitmap != null && !resumeCoverBitmap.isRecycled()) {
                attachEditorForCover();
            }
        } catch (Exception error) {
            android.util.Log.w("GpuiActivity", "showResumeCover failed", error);
        }
    }

    /**
     * Re-attaches the resume cover as an activity panel window: the panel's
     * frame is the decor view's edge-to-edge frame, so the captured frame
     * covers the notch/status-bar strip too. Re-attaching resets window
     * stacking, so the display-only editor is re-added afterwards to stay on
     * top of the cover.
     */
    private void upgradeResumeCoverToPanel() {
        WindowManager wm = getWindow().getWindowManager();
        if (wm == null || resumeCoverView == null) {
            return;
        }
        try {
            wm.removeView(resumeCoverView);
            resumeCoverParams.type =
                    WindowManager.LayoutParams.TYPE_APPLICATION_PANEL;
            resumeCoverParams.token = getWindow().getDecorView().getWindowToken();
            wm.addView(resumeCoverView, resumeCoverParams);
            resumeCoverUsedPanel = true;
            scheduleResumeCoverCalibration();
            // Re-stack the display-only editor above the re-added cover.
            if (nativeEditorAttached && textMenuEditor != null
                    && nativeEditorWindowManager != null
                    && nativeEditorWindowParams != null) {
                try {
                    nativeEditorWindowManager.removeView(textMenuEditor);
                    nativeEditorWindowManager.addView(
                            textMenuEditor, nativeEditorWindowParams);
                } catch (Exception error) {
                    android.util.Log.w(
                            "GpuiActivity", "resumeCover editor restack failed", error);
                }
            }
        } catch (Exception error) {
            android.util.Log.w("GpuiActivity", "upgradeResumeCoverToPanel failed", error);
        }
    }

    /**
     * Self-calibrates the captured frame's position: measures where the
     * cover window actually landed on screen and shifts the bitmap by the
     * difference, so its content matches the live UI 1:1 even if the window
     * frame starts below the status bar.
     */
    private void scheduleResumeCoverCalibration() {
        if (resumeCoverView == null) {
            return;
        }
        resumeCoverView.post(() -> {
            if (!resumeCoverAttached || resumeCoverView == null
                    || resumeCoverView.getChildCount() == 0) {
                return;
            }
            int[] coverOrigin = new int[2];
            resumeCoverView.getLocationOnScreen(coverOrigin);
            int shiftX = resumeCoverOriginX - coverOrigin[0];
            int shiftY = resumeCoverOriginY - coverOrigin[1];
            android.view.View frame = resumeCoverView.getChildAt(0);
            android.widget.FrameLayout.LayoutParams params =
                    (android.widget.FrameLayout.LayoutParams) frame.getLayoutParams();
            if (shiftX != 0 || shiftY != 0
                    || params.leftMargin != 0 || params.topMargin != 0) {
                params.leftMargin = shiftX;
                params.topMargin = shiftY;
                frame.setLayoutParams(params);
            }
        });
    }

    /**
     * Re-attaches the editor in display-only mode while the cover shows the
     * captured last frame: it keeps the input's text/placeholder/caret visible
     * together with the rest of the UI, but cannot be touched or focused until
     * the real GPUI frame is revealed and the parked editor op restores the
     * live geometry and interactive flags.
     */
    private void attachEditorForCover() {
        if (textMenuEditor == null
                || nativeEditorAttached
                || !editorWasAttachedAtStop
                || nativeEditorWindowManager == null
                || nativeEditorWindowParams == null) {
            return;
        }
        editorWasAttachedAtStop = false;
        nativeEditorWindowParams.alpha = 1f;
        nativeEditorWindowParams.flags |=
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
                        | WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE;
        textMenuEditor.setVisibility(View.VISIBLE);
        if (resumeEditorText != null) {
            // hideTextMenuEditor clears the mirror text; restore the frozen
            // content so the covered editor matches the captured frame.
            textMenuEditor.setText(resumeEditorText, EditText.BufferType.EDITABLE);
            textMenuEditor.setSelection(resumeEditorText.length());
        }
        try {
            nativeEditorWindowManager.addView(textMenuEditor, nativeEditorWindowParams);
            nativeEditorAttached = true;
        } catch (Exception error) {
            android.util.Log.w("GpuiActivity", "attachEditorForCover failed", error);
        }
    }

    /**
     * Copies the window's last rendered frame (GPUI content only; the editor
     * is a separate panel window and is not included) into a bitmap used as
     * the resume cover background. Runs at onPause while the surface is still
     * valid. On failure the cover falls back to the theme background color.
     */
    private void captureResumeCover() {
        android.view.View decor = getWindow() != null ? getWindow().getDecorView() : null;
        if (decor == null || decor.getWidth() <= 0 || decor.getHeight() <= 0) {
            return;
        }
        try {
            int[] onScreen = new int[2];
            decor.getLocationOnScreen(onScreen);
            resumeCoverOriginX = onScreen[0];
            resumeCoverOriginY = onScreen[1];
            if (resumeCoverBitmap != null) {
                resumeCoverBitmap.recycle();
                resumeCoverBitmap = null;
            }
            android.graphics.Bitmap capture = android.graphics.Bitmap.createBitmap(
                    decor.getWidth(),
                    decor.getHeight(),
                    android.graphics.Bitmap.Config.ARGB_8888);
            android.view.PixelCopy.request(
                    getWindow(),
                    new android.graphics.Rect(0, 0, capture.getWidth(), capture.getHeight()),
                    capture,
                    result -> {
                        if (result == android.view.PixelCopy.SUCCESS) {
                            resumeCoverBitmap = capture;
                            android.util.Log.d(
                                    "ResumeCover",
                                    "capture ok origin=(" + resumeCoverOriginX + ","
                                            + resumeCoverOriginY + ") size="
                                            + capture.getWidth() + "x" + capture.getHeight()
                                            + " decor=" + decor.getWidth() + "x"
                                            + decor.getHeight());
                        } else {
                            android.util.Log.w(
                                    "ResumeCover", "capture failed result=" + result);
                            capture.recycle();
                        }
                    },
                    new android.os.Handler(android.os.Looper.getMainLooper()));
        } catch (Throwable error) {
            android.util.Log.w("GpuiActivity", "captureResumeCover failed", error);
        }
    }

    /** Called by Rust once the first resumed GPUI frame has been submitted. */
    public void gpuiHideResumeCover() {
        runOnUiThread(() -> {
            resumeCoverNeeded = false;
            if (resumeCoverAttached && resumeCoverView != null) {
                // The cover shows the captured last frame and the live frame
                // beneath is pixel-identical, so remove it instantly: a fade
                // would only blend two copies of the same UI.
                try {
                    getWindow().getWindowManager().removeView(resumeCoverView);
                } catch (Exception error) {
                    android.util.Log.w("GpuiActivity", "hideResumeCover failed", error);
                }
                resumeCoverAttached = false;
            }
            if (resumeCoverBitmap != null) {
                resumeCoverBitmap.recycle();
                resumeCoverBitmap = null;
            }
            // Flush the parked editor op: restores the live geometry and the
            // interactive flags (or hides the editor when this page has none).
            try {
                nativeOnResumeCoverFadeStart();
            } catch (Throwable error) {
                android.util.Log.w("GpuiActivity", "resumeCoverReveal failed", error);
            }
        });
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        if (textMenuEditor != null
                && textMenuEditor.getVisibility() == View.VISIBLE
                && textMenuEditor.hasFocus()
                && event.getKeyCode() != KeyEvent.KEYCODE_BACK) {
            if (textMenuEditor.dispatchKeyEvent(event)) {
                return true;
            }
        }
        return super.dispatchKeyEvent(event);
    }

    /**
     * Called from the native event loop when the system editor owns input.
     * Software keyboards send delete as a raw key event; without this handoff
     * GPUI consumes it and the native EditText keeps the stale text that is
     * committed on blur.
     */
    public void gpuiDispatchNativeEditorKey(int action, int keyCode, int metaState) {
        runOnUiThread(() -> {
            if (textMenuEditor == null
                    || textMenuEditor.getVisibility() != View.VISIBLE
                    || !textMenuEditor.hasFocus()) {
                return;
            }
            long now = android.os.SystemClock.uptimeMillis();
            KeyEvent event = new KeyEvent(now, now, action, keyCode, 0, metaState);
            textMenuEditor.dispatchKeyEvent(event);
        });
    }

    private void dispatchDeepLink(Intent intent) {
        if (intent != null && intent.getDataString() != null) {
            nativeOnDeepLink(intent.getDataString());
        }
    }

    /** Called by Rust; safely moves focus and IME operations to the UI thread. */
    public void gpuiShowKeyboard(int inputType) {
        runOnUiThread(() -> {
            // While the native editor takeover is active, the IME must stay
            // connected to it instead of stealing focus for the GPUI shadow
            // input view.
            if (textMenuEditor != null && textMenuEditor.getVisibility() == View.VISIBLE) {
                requestImeForView(textMenuEditor, true);
                return;
            }
            inputView.setVisibility(View.VISIBLE);
            inputView.setFocusable(true);
            inputView.setFocusableInTouchMode(true);
            inputView.setInputType(inputType);
            inputView.requestFocus();
            showImeForView(inputView, true);
            inputView.postOnAnimation(() -> {
                if (inputView.hasFocus()) {
                    showImeForView(inputView, false);
                }
            });
        });
    }

    private void showImeForView(View view, boolean restartInput) {
        if (view == null || !view.isAttachedToWindow()) {
            return;
        }
        if (view == textMenuEditor && !imeVisible) {
            hideNativeEditorBeforeImePreview();
        }
        if (!view.hasFocus()) {
            view.requestFocus();
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = view.getWindowInsetsController();
            if (controller != null) {
                controller.show(WindowInsets.Type.ime());
            }
        }
        InputMethodManager manager =
                (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        if (manager != null) {
            if (restartInput) {
                manager.restartInput(view);
            }
            manager.showSoftInput(view, InputMethodManager.SHOW_IMPLICIT);
        }
    }

    private void requestImeForView(View view, boolean restartInput) {
        showImeForView(view, restartInput);
        view.postOnAnimation(() -> {
            if (!imeVisible && view.isAttachedToWindow() && view.hasFocus()) {
                showImeForView(view, false);
            }
        });
    }

    /** Called by Rust; safely hides the software keyboard on the UI thread. */
    public void gpuiHideKeyboard() {
        runOnUiThread(() -> {
            if (nativeCookiePanelAttached && nativeCookiePanel != null) {
                nativeCookieEditor.clearFocus();
                InputMethodManager panelManager =
                        (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
                if (panelManager != null) {
                    panelManager.hideSoftInputFromWindow(
                            nativeCookiePanel.getWindowToken(), 0);
                }
            }
            if (imeVisible || imeInsetsAnimating) {
                hideNativeEditorBeforeImePreview();
            }
            blurTextMenuEditor();
            InputMethodManager manager =
                    (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (manager != null) {
                manager.hideSoftInputFromWindow(inputView.getWindowToken(), 0);
            }
        });
    }

    /** Called by Rust; ends the editor's focus/IME session after a GPUI blur. */
    public void gpuiBlurNativeEditor() {
        runOnUiThread(() -> blurTextMenuEditor(true));
    }

    /** Called by Rust when an external Clear action owns the next value. */
    public void gpuiClearNativeEditorText() {
        runOnUiThread(() -> {
            if (textMenuEditor == null || !nativeEditorAttached) {
                return;
            }
            textMenuEditor.cancelPendingMirrorTextNotify();
            mirrorSyncing = true;
            textMenuEditor.setText("", EditText.BufferType.EDITABLE);
            mirrorSyncing = false;
            nativeEditorSnapshot = "";
            textMenuEditor.lastMirrorSentText = "";
            textMenuEditor.flushLineRangesNotify();
        });
    }

    /** Called by Rust after a reset; mirror callbacks must use its session. */
    public void gpuiSetNativeEditorSession(long session) {
        runOnUiThread(() -> nativeEditorSession = session);
    }

    /** IME bottom inset reported by a snapshot, matching applyWindowInsets. */
    private static int imeBottomOf(WindowInsets insets) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            return insets.getInsets(WindowInsets.Type.ime()).bottom;
        }
        return Math.max(
                0,
                insets.getSystemWindowInsetBottom() - insets.getStableInsetBottom());
    }

    /**
     * Apply a WindowInsets snapshot to Java-side IME state and the editor
     * window.
     *
     * <p>When the Cookie dialog follows the IME, the editor window and GPUI
     * see the same snapshot in the same callback so the card and the native
     * editor move on the same frame.</p>
     */
    private void applyWindowInsets(WindowInsets insets, boolean animationFrame) {
        int left;
        int top;
        int right;
        int bottom;
        int imeLeft = 0;
        int imeTop = 0;
        int imeRight = 0;
        int imeBottom = 0;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Insets bars = insets.getInsets(
                    WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            Insets ime = insets.getInsets(WindowInsets.Type.ime());
            left = bars.left;
            top = bars.top;
            right = bars.right;
            bottom = bars.bottom;
            imeLeft = ime.left;
            imeTop = ime.top;
            imeRight = ime.right;
            imeBottom = ime.bottom;
        } else {
            left = insets.getSystemWindowInsetLeft();
            top = insets.getSystemWindowInsetTop();
            right = insets.getSystemWindowInsetRight();
            bottom = insets.getSystemWindowInsetBottom();
            // On API 26-29 the system reports the keyboard and the
            // navigation bar through the same bottom inset. The stable inset
            // exposes only the keyboard-sized difference.
            imeBottom = Math.max(
                    0,
                    insets.getSystemWindowInsetBottom() - insets.getStableInsetBottom());
        }
        int previousImeBottom = imeBottomPixels;
        imeVisible = imeBottom > 0;
        imeBottomPixels = imeBottom;
        lastInsetsTop = top;
        nativeCookieBarsLeft = left;
        nativeCookieBarsRight = right;
        if (nativeCookiePanelAttached) {
            // The panel window is positioned by hand, so it needs the
            // Activity's keyboard height; the panel's own insets would depend
            // on that very position.
            int previousCookieIme = nativeCookieActivityIme;
            nativeCookieActivityIme = imeBottom;
            nativeCookieBarsTop = top;
            // A back press while the keyboard is open is consumed by the IME,
            // so the panel never sees the key. The keyboard collapsing under
            // the editor is the same gesture: drop focus with it, exactly
            // like the panel's own back handling, and leave the card open.
            if (previousCookieIme > 0
                    && imeBottom == 0
                    && nativeCookieEditor != null
                    && nativeCookieEditor.hasFocus()) {
                nativeCookieEditor.clearFocus();
                applyCookieEditorBackground();
            }
            layoutNativeCookiePanel();
        }
        android.util.Log.d(
                "GPUIInsets",
                "L=" + left + " T=" + top + " R=" + right + " B=" + bottom
                        + " ime=(" + imeLeft + "," + imeTop + ","
                        + imeRight + "," + imeBottom + ")"
                        + (animationFrame ? " anim" : ""));
        // Keyboards on some ROMs report only the final IME height, without
        // Deliver every snapshot exactly as the system reports it. Keyboards
        // that provide inset-animation frames arrive here per frame; the rest
        // publish one final height, and both surfaces move to it on their own
        // next frame. No synthesized timing: the editor window follows the
        // rect Rust paints, so the two never move relative to each other.
        deliverNativeInsets(
                left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom, previousImeBottom);
    }

    /** Hand an inset snapshot to Rust through the Cookie handoff pipeline. */
    private void deliverNativeInsets(
            int left,
            int top,
            int right,
            int bottom,
            int imeLeft,
            int imeTop,
            int imeRight,
            int imeBottom,
            int previousImeBottom) {
        if (nativeEditorImeFollow) {
            if (nativeEditorImePreview) {
                if (imeBottom != previousImeBottom) {
                    boolean hiddenBeforeInsetChange = nativeEditorImePreviewAnimating;
                    beginNativeEditorImePreview();
                    if (hiddenBeforeInsetChange) {
                        dispatchNativeInsetsChanged(
                                left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
                        scheduleNativeEditorImePreviewFinish(imeBottom);
                    } else {
                        scheduleNativeEditorImePreviewInsets(
                                left,
                                top,
                                right,
                                bottom,
                                imeLeft,
                                imeTop,
                                imeRight,
                                imeBottom);
                    }
                } else {
                    dispatchNativeInsetsChanged(
                            left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
                }
            } else {
                // Hand GPUI the inset in the same callback that moved the
                // editor window. Deferring it behind the WindowManager
                // traversal makes the card visibly trail the native editor
                // through the whole keyboard animation.
                dispatchNativeInsetsChanged(
                        left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
            }
        } else {
            dispatchNativeInsetsChanged(
                    left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
        }
    }

    private void dispatchNativeInsetsChanged(
            int left,
            int top,
            int right,
            int bottom,
            int imeLeft,
            int imeTop,
            int imeRight,
            int imeBottom) {
        nativeLayoutImeBottom = imeBottom;
        nativeOnInsetsChanged(
                left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
    }

    /** Translate the attached native editor window with its moving dialog. */
    public void gpuiTranslateNativeEditor(float deltaY) {
        runOnUiThreadBlocking(() -> {
            float delta = deltaY * getResources().getDisplayMetrics().density;
            translateNativeEditorWindow(delta);
        });
    }

    /**
     * Let Java hand every IME snapshot to GPUI while the Cookie dialog is
     * centred. The card reflows with the IME, and the editor window follows
     * the rect Rust paints for it on each frame.
     */
    public void gpuiSetNativeEditorImeFollow(boolean follow) {
        runOnUiThread(() -> nativeEditorImeFollow = follow);
    }

    /**
     * Enable the Android-only Cookie IME handoff. While this is active, the
     * GPUI dialog owns the input's visual representation during keyboard
     * animations; the real EditText stays mounted but transparent and is
     * revealed at its final position after Rust submits the final frame.
     */
    public void gpuiSetNativeEditorImePreview(boolean preview) {
        runOnUiThreadBlocking(() -> {
            android.util.Log.d("GPUIInsetsAnim", "preview=" + preview);
            if (nativeEditorImePreview == preview) {
                return;
            }
            nativeEditorImePreview = preview;
            if (!preview) {
                nativeEditorImePreviewAnimating = false;
                nativeEditorImePreviewShowPending = false;
                nativeOnNativeEditorImePreviewVisible(false);
            }
            applyEditorBackground();
            if (!preview && !nativeEditorWarmup) {
                setNativeEditorWindowAlpha(1.0f);
                setNativeEditorContentAlpha(1.0f);
            }
        });
    }

    /**
     * Called from Rust after the final IME-animation frame has submitted.
     * The GPUI copy is already at the final position, so revealing the native
     * editor cannot expose a trailing or leading frame.
     */
    public void gpuiShowNativeEditorAfterImeAnimation() {
        runOnUiThreadBlocking(() -> {
            android.util.Log.d(
                    "GPUIInsetsAnim",
                    "showAfter pending=" + nativeEditorImePreviewShowPending
                            + " attached=" + nativeEditorAttached);
            if (!nativeEditorImePreview || !nativeEditorImePreviewShowPending) {
                return;
            }
            nativeEditorImePreviewShowPending = false;
            nativeEditorImePreviewAnimating = false;
            if (!nativeEditorAttached
                    || nativeEditorWindowParams == null
                    || nativeEditorWindowManager == null
                    || textMenuEditor == null) {
                return;
            }
            setNativeEditorWindowAlpha(1.0f);
            setNativeEditorContentAlpha(1.0f);
            inputView.postOnAnimation(() -> inputView.postOnAnimation(
                    () -> nativeOnNativeEditorImePreviewVisible(false)));
        });
    }

    /**
     * Some keyboards, including the one on the test device, only publish the
     * final IME inset and never dispatch WindowInsetsAnimation progress. Hide
     * the native editor as soon as any inset change arrives, let the GPUI
     * preview move with the card, then restore the native editor after the
     * inset has stayed stable for a few frames.
     */
    private void hideNativeEditorBeforeImePreview() {
        if (!nativeEditorImePreview
                || !nativeEditorAttached
                || textMenuEditor == null
                || textMenuEditor.getVisibility() != View.VISIBLE) {
            return;
        }
        if (!nativeEditorImePreviewAnimating) {
            nativeEditorImePreviewAnimating = true;
            nativeEditorImePreviewShowPending = false;
            nativeOnNativeEditorImePreviewVisible(true);
            setNativeEditorContentAlpha(0.0f);
        }
    }

    private void beginNativeEditorImePreview() {
        if (!nativeEditorImePreview
                || !nativeEditorAttached
                || textMenuEditor == null
                || inputView == null) {
            return;
        }
        if (!nativeEditorImePreviewAnimating) {
            nativeEditorImePreviewAnimating = true;
            nativeEditorImePreviewShowPending = false;
            nativeOnNativeEditorImePreviewVisible(true);
            setNativeEditorContentAlpha(0.0f);
        }
    }

    private void scheduleNativeEditorImePreviewFinish(int imeBottom) {
        int sequence = ++nativeEditorImePreviewSettleSequence;
        inputView.postDelayed(
                () -> finishNativeEditorImePreview(imeBottom, sequence),
                48);
    }

    private void scheduleNativeEditorImePreviewInsets(
            int left,
            int top,
            int right,
            int bottom,
            int imeLeft,
            int imeTop,
            int imeRight,
            int imeBottom) {
        int sequence = ++nativeEditorImePreviewSettleSequence;
        // Alpha=0 reaches WindowManager on the next traversal. Let that
        // traversal and its composition land before moving the GPUI copy, so
        // the old native frame can never remain visible beside the new one.
        inputView.postOnAnimation(() -> inputView.postOnAnimation(() -> {
            if (sequence != nativeEditorImePreviewSettleSequence) {
                return;
            }
            dispatchNativeInsetsChanged(
                    left, top, right, bottom, imeLeft, imeTop, imeRight, imeBottom);
            scheduleNativeEditorImePreviewFinish(imeBottom);
        }));
    }

    private void finishNativeEditorImePreview(int imeBottom, int sequence) {
        if (!nativeEditorImePreview
                || !nativeEditorImePreviewAnimating
                || sequence != nativeEditorImePreviewSettleSequence
                || imeBottomPixels != imeBottom) {
            return;
        }
        nativeEditorImePreviewShowPending = true;
        nativeOnNativeEditorImeAnimationEnd();
    }

    /** Hide the editor during the Cookie dialog's first GPUI layout passes. */
    public void gpuiSetNativeEditorWarmup(boolean warmup) {
        runOnUiThreadBlocking(() -> {
            nativeEditorWarmup = warmup;
            if (!nativeEditorAttached
                    || nativeEditorWindowParams == null
                    || nativeEditorWindowManager == null
                    || textMenuEditor == null) {
                return;
            }
            float alpha = warmup ? 0.0f : 1.0f;
            if (nativeEditorWindowParams.alpha == alpha) {
                return;
            }
            nativeEditorWindowParams.alpha = alpha;
            nativeEditorWindowManager.updateViewLayout(
                    textMenuEditor, nativeEditorWindowParams);
        });
    }

    /** Move the attached editor window by a physical-pixel delta. */
    private void translateNativeEditorWindow(float delta) {
        if (delta == 0.0f
                || !nativeEditorAttached
                || nativeEditorWindowParams == null
                || textMenuEditor == null) {
            return;
        }
        // Keep the position as a float across the IME animation's small
        // per-frame steps; rounding each step independently drifts.
        nativeEditorWindowY += delta;
        int y = Math.round(nativeEditorWindowY);
        if (y == nativeEditorWindowParams.y) {
            return;
        }
        nativeEditorWindowParams.y = y;
        nativeEditorWindowManager.updateViewLayout(
                textMenuEditor, nativeEditorWindowParams);
    }

    private void setNativeEditorWindowAlpha(float alpha) {
        if (!nativeEditorAttached
                || nativeEditorWindowParams == null
                || nativeEditorWindowManager == null
                || textMenuEditor == null
                || Float.compare(nativeEditorWindowParams.alpha, alpha) == 0) {
            return;
        }
        nativeEditorWindowParams.alpha = alpha;
        nativeEditorWindowManager.updateViewLayout(
                textMenuEditor, nativeEditorWindowParams);
    }

    private void setNativeEditorContentAlpha(float alpha) {
        if (textMenuEditor == null
                || Float.compare(textMenuEditor.getAlpha(), alpha) == 0) {
            return;
        }
        textMenuEditor.setAlpha(alpha);
    }

    /**
     * Called by Rust when a dialog is dismissed; drop its unsaved draft.
     */
    public void gpuiResetNativeEditorText(String text, long session) {
        runOnUiThread(() -> {
            if (textMenuEditor == null) {
                return;
            }
            String saved = text == null ? "" : text;
            nativeEditorSession = session;
            textMenuEditor.cancelPendingMirrorTextNotify();
            textMenuEditor.lastMirrorSentText = null;
            mirrorSyncing = true;
            textMenuEditor.setText(saved, EditText.BufferType.EDITABLE);
            mirrorSyncing = false;
            nativeEditorSnapshot = saved;
            textMenuEditor.lastMirrorSentText = saved;
            textMenuEditor.setSelection(saved.length());
            textMenuEditor.visualScrollTop = 0;
            textMenuEditor.resetNotifiedScroll();
            if (nativeEditorAttached) {
                textMenuEditor.flushLineRangesNotify();
            }
        });
    }

    /** Real native editor taking over the focused GPUI input so the system
     *  draws its own keyboard, caret, selection handles and edit menu. */
    private MirrorEditText textMenuEditor;
    private WindowManager nativeEditorWindowManager;
    /** The Cookie dialog reflows with the IME; its editor follows the card. */
    private boolean nativeEditorImeFollow;
    /** Attach the next editor window invisibly and reveal it without another addView. */
    private boolean nativeEditorWarmup;
    private int imeBottomPixels;
    /** Editor window top in physical pixels, unrounded across IME frames. */
    private float nativeEditorWindowY;
    /** IME height used by the latest layout delivered to Rust. */
    private int nativeLayoutImeBottom = Integer.MIN_VALUE;
    /** Latest system-bar top inset reported to the Activity window. */
    private int lastInsetsTop;
    private boolean imeInsetsAnimating;
    private android.widget.FrameLayout resumeCoverView;
    private WindowManager.LayoutParams resumeCoverParams;
    private boolean resumeCoverAttached;
    /** Whether the cover is a full-screen activity panel (covers the notch). */
    private boolean resumeCoverUsedPanel;
    private boolean resumeCoverNeeded;
    /** Last rendered frame captured at onPause; the resume cover background. */
    private android.graphics.Bitmap resumeCoverBitmap;
    /** On-screen origin of the captured frame (window content area). */
    private int resumeCoverOriginX;
    private int resumeCoverOriginY;
    /** Whether the editor panel was attached when the app was stopped. */
    private boolean editorWasAttachedAtStop;
    /** Editor text at stop, restored on the covered display-only editor. */
    private String resumeEditorText;
    private WindowManager.LayoutParams nativeEditorWindowParams;
    private boolean nativeEditorAttached;
    private boolean nativeEditorFocusableMode;
    private boolean nativeEditorFocusRequestPending;
    private boolean nativeEditorImePreview;
    private boolean nativeEditorImePreviewAnimating;
    private boolean nativeEditorImePreviewShowPending;
    private int nativeEditorImePreviewSettleSequence;
    /** Guards native callbacks while programmatically updating the mirror. */
    private boolean mirrorSyncing;
    /** Last per-frame editor styling/layout applied (debounce guards). */
    private int editorLeft = -1;
    private int editorTop = -1;
    private int editorWidth = -1;
    private int editorHeight = -1;
    private int editorPaddingLeft = -1;
    private int editorPaddingRight = -1;
    private int editorPaddingTop = -1;
    /**
     * Extra top padding (px) discovered by comparing the live TextView
     * baseline against GPUI's. Font metrics alone cannot predict it because
     * {@link LineHeightSpan} rewrites the first line's ascent, so the editor
     * calibrates itself once and reuses the result until the font changes.
     */
    private int editorBaselineCorrectionPx = 0;
    private int editorContentWidth = -1;
    private int editorInputType = -1;
    private int editorBgColor = 0;
    private int editorTextColor = 0;
    private int editorBorderColor = 0;
    private int editorFocusBorderColor = 0;
    private float editorBorderWidth = 1.0f;
    private float editorCornerRadius = -1;
    private float editorTextSize = -1;
    private float editorLineHeight = -1;
    private Rect editorClipBounds;
    private String editorHint;
    private int editorHintColor = 0;
    private int lastMirrorSelectionStart = -1;
    private int lastMirrorSelectionEnd = -1;
    private float editorTargetLineHeightPx = -1;
    private boolean imeVisible;
    private final ExactLineHeightSpan exactLineHeightSpan = new ExactLineHeightSpan();

    /**
     * Android's per-line fallback fonts can report much taller metrics than
     * the Paint base metrics (CJK fallback commonly adds 30+ px). A normal
     * setLineSpacing adjustment therefore cannot keep EditText rows aligned
     * with GPUI. This span pins every line box to the exact GPUI height.
     */
    private static final class ExactLineHeightSpan
            implements LineHeightSpan.WithDensity {
        private int lineHeightPx;

        boolean setLineHeightPx(int value) {
            if (lineHeightPx == value) {
                return false;
            }
            lineHeightPx = value;
            return true;
        }

        @Override
        public void chooseHeight(
                CharSequence text,
                int start,
                int end,
                int spanstartv,
                int lineHeight,
                Paint.FontMetricsInt fontMetrics,
                TextPaint paint) {
            chooseHeight(text, start, end, spanstartv, lineHeight, fontMetrics);
        }

        @Override
        public void chooseHeight(
                CharSequence text,
                int start,
                int end,
                int spanstartv,
                int lineHeight,
                Paint.FontMetricsInt fontMetrics) {
            if (lineHeightPx <= 0) {
                return;
            }
            int ascent = -Math.round(lineHeightPx * 0.8f);
            int descent = lineHeightPx + ascent;
            fontMetrics.ascent = ascent;
            fontMetrics.top = ascent;
            fontMetrics.descent = descent;
            fontMetrics.bottom = descent;
            fontMetrics.leading = 0;
        }
    }
    /**
     * Latest text observed on the Android UI thread. JNI commits run on the
     * native GPUI thread, where reading {@link EditText} state directly is
     * unsynchronized and can return a stale value after select-all/delete.
     */
    private volatile String nativeEditorSnapshot;
    private volatile long nativeEditorSession;

    /**
     * Called by Rust; hands the focused GPUI input over to a real native
     * EditText overlaid on the input's bounds. The system owns the keyboard,
     * caret, selection handles, and edit menu; edits sync back through
     * {@code nativeMirrorTextChanged}/{@code nativeMirrorSelectionChanged}.
     */
    public void gpuiShowNativeEditor(
            String text,
            int selStart,
            int selEnd,
            float boundsX,
            float boundsY,
            float boundsW,
            float boundsH,
            float textOriginX,
            float textOriginY,
            float fontSize,
            float wrapWidth,
            float lineHeight,
            int textColor,
            int bgColor,
            int borderColor,
            int focusBorderColor,
            float borderWidth,
            String placeholder,
            int placeholderColor,
            String lineRanges,
            float cornerRadius,
            float clipX,
            float clipY,
            float clipWidth,
            float clipHeight,
            boolean hasClip,
            boolean multiline,
            boolean initial,
            boolean forceTextSync) {
        runOnUiThreadBlocking(() -> {
            android.util.Log.d(
                    "GPUI_NATIVE_INPUT",
                    "show initial=" + initial
                            + " force=" + forceTextSync
                            + " attached=" + nativeEditorAttached
                            + " visibility=" + (textMenuEditor == null
                                    ? "null"
                                    : textMenuEditor.getVisibility()));
            if (inputView == null || !inputView.isAttachedToWindow() || textMenuEditor == null) {
                return;
            }
            if (nativeCookiePanelAttached) {
                // The Cookie dialog owns the keyboard through its own panel
                // window; a stale GPUI editor op must not steal that focus.
                return;
            }
            float density = getResources().getDisplayMetrics().density;
            int[] anchorLocation = new int[2];
            inputView.getLocationOnScreen(anchorLocation);
            final int left = anchorLocation[0] + Math.round(boundsX * density);
            final int top = anchorLocation[1] + Math.round(boundsY * density);
            final int width = Math.max(1, Math.round(boundsW * density));
            final int height = Math.max(1, Math.round(boundsH * density));
            final Rect clipBounds;
            if (hasClip) {
                int editorLocalLeft =
                        Math.round(clipX * density) - Math.round(boundsX * density);
                int editorLocalTop =
                        Math.round(clipY * density) - Math.round(boundsY * density);
                int editorLocalRight =
                        Math.round((clipX + clipWidth) * density)
                                - Math.round(boundsX * density);
                int editorLocalBottom =
                        Math.round((clipY + clipHeight) * density)
                                - Math.round(boundsY * density);
                clipBounds = new Rect(
                        editorLocalLeft,
                        editorLocalTop,
                        Math.max(editorLocalLeft, editorLocalRight),
                        Math.max(editorLocalTop, editorLocalBottom));
            } else {
                clipBounds = null;
            }
            if (!java.util.Objects.equals(clipBounds, editorClipBounds)) {
                editorClipBounds = clipBounds;
                textMenuEditor.setClipBounds(clipBounds);
                textMenuEditor.invalidate();
            }
            // Per-frame refreshes repeat the same values; only touch the view
            // when something changed so the IME connection and scroll state
            // are never disrupted.
            boolean geometryChanged =
                    left != editorLeft || top != editorTop
                            || width != editorWidth || height != editorHeight;
            if (geometryChanged) {
                editorLeft = left;
                editorTop = top;
                editorWidth = width;
                editorHeight = height;
                if (textMenuEditor.getVisibility() == View.VISIBLE) {
                    final int previousScroll = textMenuEditor.visualScrollTop;
                    textMenuEditor.postOnAnimation(() -> {
                        if (!nativeEditorAttached || textMenuEditor.scrollingText) {
                            return;
                        }
                        textMenuEditor.scrollTo(0, previousScroll);
                        textMenuEditor.revealSelection(
                                textMenuEditor.getSelectionEnd());
                    });
                }
            }
            int newInputType = multiline
                    ? (android.text.InputType.TYPE_CLASS_TEXT
                            | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE)
                    : android.text.InputType.TYPE_CLASS_TEXT;
            if (newInputType != editorInputType) {
                textMenuEditor.setInputType(newInputType);
                textMenuEditor.setImeOptions(
                        EditorInfo.IME_ACTION_NONE
                                | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                                | EditorInfo.IME_FLAG_NO_FULLSCREEN);
                editorInputType = newInputType;
            }
            float newTextSize = fontSize * density;
            if (Math.abs(newTextSize - editorTextSize) > 0.01f) {
                textMenuEditor.setTextSize(
                        android.util.TypedValue.COMPLEX_UNIT_PX, newTextSize);
                editorTextSize = newTextSize;
                editorLineHeight = -1;
                editorBaselineCorrectionPx = 0;
            }
            float newLineHeight = multiline ? lineHeight * density : 0.0f;
            if (Math.abs(newLineHeight - editorLineHeight) > 0.01f) {
                editorLineHeight = newLineHeight;
                editorTargetLineHeightPx = newLineHeight;
                editorBaselineCorrectionPx = 0;
                applyExactLineHeight();
            }
            // Measure after the text size and line spacing above have been
            // applied: the metrics are font-size dependent, so sampling them
            // earlier yields the previous editor's ascent.
            final int paddingLeft =
                    Math.max(0, Math.round((textOriginX - boundsX) * density));
            // `textOriginY` is GPUI's first-line baseline. TextView draws its
            // first baseline at `paddingTop + baselineFromContentTop`, where
            // that offset comes from the live font metrics (and includes the
            // font padding). Solving for the padding keeps both renderers on
            // the same pixel row at every density, font size, and font scale,
            // instead of compensating with a fixed number of dp.
            final float targetBaselinePx = (textOriginY - boundsY) * density;
            float baselineFromContentTop = 0.0f;
            if (multiline) {
                Paint.FontMetricsInt metrics =
                        textMenuEditor.getPaint().getFontMetricsInt();
                baselineFromContentTop = textMenuEditor.getIncludeFontPadding()
                        ? -metrics.top
                        : -metrics.ascent;
            }
            final int paddingTop = Math.max(
                    0,
                    multiline
                            ? Math.round(targetBaselinePx - baselineFromContentTop)
                                    + editorBaselineCorrectionPx
                            : Math.round(targetBaselinePx));
            final int availableWidth = Math.max(1, width - paddingLeft);
            final int contentWidth =
                    multiline && wrapWidth > 0.0f
                            ? Math.min(
                                    availableWidth,
                                    Math.max(1, Math.round(wrapWidth * density)))
                            : availableWidth;
            final int paddingRight =
                    Math.max(0, availableWidth - contentWidth);
            if (paddingLeft != editorPaddingLeft
                    || paddingRight != editorPaddingRight
                    || paddingTop != editorPaddingTop
                    || contentWidth != editorContentWidth) {
                textMenuEditor.setPadding(
                        paddingLeft, paddingTop, paddingRight, paddingTop);
                editorPaddingLeft = paddingLeft;
                editorPaddingRight = paddingRight;
                editorPaddingTop = paddingTop;
                editorContentWidth = contentWidth;
            }
            if (multiline) {
                // Self-calibrate against the view's own baseline. The
                // `LineHeightSpan` applied below rewrites the first line's
                // ascent, so no static formula can predict where TextView
                // puts its glyphs; one measured correction is exact for any
                // density, font size, and system font scale.
                final int desiredBaselinePx = Math.round(targetBaselinePx);
                final int correctionWhenPosted = editorBaselineCorrectionPx;
                textMenuEditor.post(() -> {
                    if (!nativeEditorAttached || textMenuEditor.getVisibility() != View.VISIBLE) {
                        return;
                    }
                    int nativeBaseline = textMenuEditor.getBaseline();
                    if (nativeBaseline <= 0) {
                        return;
                    }
                    int delta = desiredBaselinePx - nativeBaseline;
                    // Ignore implausibly large deltas: they only appear when
                    // the measurement races a card move, and applying them
                    // would make the correction drift.
                    if (Math.abs(delta) >= 1
                            && Math.abs(delta) <= 32
                            && editorBaselineCorrectionPx == correctionWhenPosted) {
                        editorBaselineCorrectionPx += delta;
                        // Force the next pass to re-apply the padding.
                        editorPaddingTop = -1;
                    }
                });
            }
            if (bgColor != editorBgColor || textColor != editorTextColor
                    || borderColor != editorBorderColor
                    || focusBorderColor != editorFocusBorderColor
                    || Math.abs(borderWidth - editorBorderWidth) > 0.01f
                    || cornerRadius != editorCornerRadius) {
                int argbTextColor = gpuiColorToArgb(textColor);
                textMenuEditor.setTextColor(ColorStateList.valueOf(argbTextColor));
                textMenuEditor.getPaint().setColor(argbTextColor);
                editorBgColor = bgColor;
                editorTextColor = textColor;
                editorBorderColor = borderColor;
                editorFocusBorderColor = focusBorderColor;
                editorBorderWidth = borderWidth;
                editorCornerRadius = cornerRadius;
                applyEditorBackground();
                applyNativeSelectionColors(
                        isDarkBackgroundColor(gpuiColorToArgb(editorBgColor)));
            }
            String newHint = placeholder == null || placeholder.isEmpty() ? null : placeholder;
            if (newHint == null ? editorHint != null : !newHint.equals(editorHint)) {
                textMenuEditor.setHint(newHint);
                editorHint = newHint;
            }

            if (placeholderColor != editorHintColor) {
                textMenuEditor.setHintTextColor(gpuiColorToArgb(placeholderColor));
                editorHintColor = placeholderColor;
            }

            boolean wasVisible = nativeEditorAttached;
            // Seed on first display. Once mounted, the editor is the source of
            // truth while it has focus. When it is unfocused, external actions
            // such as Paste/Clear may update the GPUI value and must be mirrored
            // into the persistent native view.
            // Text that equals the last value sent upstream (or is still
            // pending upstream) is the echo of our own mirror notification,
            // not an external edit, so it must never clobber newer editor
            // content while the user is typing.
            boolean isEchoFromEditor =
                    (textMenuEditor.lastMirrorSentText != null
                            && text.contentEquals(textMenuEditor.lastMirrorSentText))
                            || (textMenuEditor.pendingMirrorText != null
                                    && text.contentEquals(textMenuEditor.pendingMirrorText));
            // While the EditText owns focus, GPUI can be one IME batch behind,
            // so ordinary mirror echoes must not replace active composition.
            // A controlled value change from outside the editor (Paste/Clear)
            // is explicitly tagged and wins even if the blur request has not
            // reached the UI thread yet.
            boolean textDiffers = !text.contentEquals(textMenuEditor.getText());
            boolean shouldSyncText = !wasVisible
                    || forceTextSync
                    || (textDiffers
                            && !textMenuEditor.hasFocus()
                            && !isEchoFromEditor);
            if (shouldSyncText) {
                boolean supersedesEditorText = wasVisible;
                boolean hadPendingMirrorText =
                        textMenuEditor.pendingMirrorText != null
                                || (textMenuEditor.lastMirrorSentText != null
                                        && !text.contentEquals(
                                                textMenuEditor.lastMirrorSentText));
                if (supersedesEditorText) {
                    // An external value change (most notably Clear) wins over
                    // any debounced snapshot produced by the previous editor
                    // contents. Otherwise that old snapshot can arrive after
                    // the clear and restore stale button state.
                    textMenuEditor.cancelPendingMirrorTextNotify();
                }
                boolean textChanged = !text.contentEquals(textMenuEditor.getText());
                if (textChanged) {
                    mirrorSyncing = true;
                    textMenuEditor.setText(
                            textWithExactLineHeight(text),
                            EditText.BufferType.EDITABLE);
                    mirrorSyncing = false;
                }
                nativeEditorSnapshot = textMenuEditor.getText().toString();
                int newSelStart = Math.min(selStart, text.length());
                int newSelEnd = Math.min(selEnd, text.length());
                if (textMenuEditor.getSelectionStart() != newSelStart
                        || textMenuEditor.getSelectionEnd() != newSelEnd) {
                    mirrorSyncing = true;
                    textMenuEditor.setSelection(newSelStart, newSelEnd);
                    mirrorSyncing = false;
                }
                if (supersedesEditorText && (textChanged || hadPendingMirrorText)) {
                    // Replace any older snapshot already queued in Rust with
                    // this controlled value so a paste cannot overtake Clear.
                    textMenuEditor.lastMirrorSentText = nativeEditorSnapshot;
                    nativeMirrorTextChanged(
                            nativeEditorSnapshot,
                            Math.max(0, textMenuEditor.getSelectionStart()),
                            Math.max(0, textMenuEditor.getSelectionEnd()),
                            nativeEditorSession);
                }
            }
            if (!wasVisible) {
                textMenuEditor.visualScrollTop = 0;
                textMenuEditor.resetNotifiedScroll();
                textMenuEditor.scheduleNativeScrollNotify();
                lastMirrorSelectionStart = -1;
                lastMirrorSelectionEnd = -1;
                textMenuEditor.setVisibility(View.VISIBLE);
                applyExactLineHeight();
                // The GPUI shadow view must not compete with the real editor
                // for focus. If it stays focusable, Android can keep the IME
                // and text-selection session attached to the shadow even
                // though the EditText is the visible interaction target.
                inputView.setVisibility(View.INVISIBLE);
                inputView.setFocusable(false);
                inputView.setFocusableInTouchMode(false);
                inputView.clearFocus();
                getWindow().getDecorView().clearFocus();
                textMenuEditor.setClickable(true);
                textMenuEditor.setFocusable(true);
                textMenuEditor.setFocusableInTouchMode(true);
                textMenuEditor.setLongClickable(true);
                textMenuEditor.setCursorVisible(true);
                textMenuEditor.setTextIsSelectable(true);
                textMenuEditor.setMovementMethod(ArrowKeyMovementMethod.getInstance());
                nativeEditorSnapshot = textMenuEditor.getText().toString();
            }
            ensureNativeEditorWindow(
                    left,
                    top,
                    width,
                    height,
                    initial || nativeEditorFocusableMode);
            if (initial) {
                textMenuEditor.setCursorVisible(true);
                textMenuEditor.setTextIsSelectable(true);
                textMenuEditor.setClickable(true);
                textMenuEditor.setLongClickable(true);
                textMenuEditor.setMovementMethod(ArrowKeyMovementMethod.getInstance());
                textMenuEditor.setFocusable(true);
                textMenuEditor.setFocusableInTouchMode(true);
                scheduleNativeEditorFocus();
            }
            textMenuEditor.notifyLineRanges();
        });
    }

    /**
     * A WindowManager child is not attached to a real ViewRoot until the
     * application-panel window has been added and traversed. Requesting focus
     * or showing the IME inline after addView() makes both requests no-op,
     * while the premature focus callback can make GPUI immediately hide the
     * editor again. Coalesce first-focus handoffs until the view is mounted.
     */
    private void scheduleNativeEditorFocus() {
        if (textMenuEditor == null || nativeEditorFocusRequestPending) {
            return;
        }
        nativeEditorFocusRequestPending = true;
        textMenuEditor.postOnAnimation(() -> {
            nativeEditorFocusRequestPending = false;
            if (textMenuEditor == null
                    || !nativeEditorAttached
                    || !nativeEditorFocusableMode
                    || !textMenuEditor.isAttachedToWindow()
                    || textMenuEditor.getVisibility() != View.VISIBLE) {
                return;
            }
            textMenuEditor.requestFocus();
            textMenuEditor.requestFocusFromTouch();
            textMenuEditor.revealSelection(textMenuEditor.getSelectionEnd());
            requestImeForView(textMenuEditor, true);
        });
    }

    private void applyEditorBackground() {
        if (textMenuEditor == null) {
            return;
        }
        float density = getResources().getDisplayMetrics().density;
        android.graphics.drawable.GradientDrawable background =
                new android.graphics.drawable.GradientDrawable();
        // Normally the editor stays transparent so page transitions can show
        // through it. During the Cookie IME handoff a GPUI copy sits below the
        // native window; an opaque background hides that copy once the real
        // editor is revealed, without changing any other input.
        background.setColor(
                nativeEditorImePreview
                        ? gpuiColorToArgb(editorBgColor)
                        : android.graphics.Color.TRANSPARENT);
        background.setCornerRadius(editorCornerRadius * density);
        int strokeColor = textMenuEditor.hasFocus()
                ? editorFocusBorderColor
                : editorBorderColor;
        background.setStroke(
                Math.max(1, Math.round(editorBorderWidth * density)),
                gpuiColorToArgb(strokeColor));
        textMenuEditor.setBackground(background);
    }

    /**
     * Host the real EditText as an application window above NativeActivity's
     * native surface. Android disables its selection controllers in panel
     * sub-windows, so this root window must use the ActivityRecord token.
     */
    private void ensureNativeEditorWindow(
            int left, int top, int width, int height, boolean focusable) {
        if (nativeEditorWindowManager == null) {
            // The Activity's local WindowManager knows its ActivityRecord
            // token and supplies it when adding a TYPE_APPLICATION window.
            nativeEditorWindowManager = getWindow().getWindowManager();
        }
        if (nativeEditorWindowManager == null) {
            return;
        }
        int flags = WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
                | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED;
        // The editor window must remain focusable for the IME, but touches
        // outside its exact bounds belong to GPUI controls underneath it
        // (buttons, dialog overlay, navigation). FLAG_NOT_TOUCH_MODAL alone
        // still lets a focused window consume the first tap as a focus
        // change; SPLIT_TOUCH and LIMITED_TOUCH_AREA keep that first gesture
        // available to the lower window without recreating the EditText.
        flags |= WindowManager.LayoutParams.FLAG_SPLIT_TOUCH;
        flags |= WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN;
        if (!focusable) {
            flags |= WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE;
        }
        if (nativeEditorWindowParams == null) {
            nativeEditorWindowParams = new WindowManager.LayoutParams(
                    width,
                    height,
                    WindowManager.LayoutParams.TYPE_APPLICATION,
                    flags,
                    android.graphics.PixelFormat.TRANSLUCENT);
            nativeEditorWindowParams.gravity = Gravity.TOP | Gravity.START;
            nativeEditorWindowParams.softInputMode =
                    WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING;
            nativeEditorWindowParams.setTitle("ReelsNativeInput");
        }
        nativeEditorWindowParams.width = width;
        nativeEditorWindowParams.height = height;
        nativeEditorWindowParams.x = left;
        int windowTop = top;
        if (nativeEditorImeFollow) {
            // Rust re-reports the editor rect on every paint, already
            // reflowed for the IME height it received. Anchoring the window
            // to that rect keeps it glued to the card's input slot; any
            // IME-derived formula drifts and makes the editor bounce inside
            // the card.
            windowTop = top;
        }
        nativeEditorWindowParams.y = windowTop;
        nativeEditorWindowY = windowTop;
        nativeEditorWindowParams.alpha = nativeEditorWarmup ? 0.0f : 1.0f;
        // Restrict touch ownership to the visible editor rect. The system
        // can then deliver a tap on a sibling button to the Activity in the
        // same gesture, while still dispatching caret, long-press and text
        // selection touches to this EditText.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            nativeEditorWindowParams.setFitInsetsTypes(0);
        }
        nativeEditorWindowParams.flags = flags;
        nativeEditorFocusableMode = focusable;
        if (!nativeEditorAttached) {
            textMenuEditor.setVisibility(View.VISIBLE);
            try {
                nativeEditorWindowManager.addView(textMenuEditor, nativeEditorWindowParams);
            } catch (WindowManager.BadTokenException error) {
                if (nativeEditorWindowParams.type
                        != WindowManager.LayoutParams.TYPE_APPLICATION) {
                    throw error;
                }
                android.util.Log.w(
                        "GpuiActivity",
                        "Application editor window rejected; falling back to panel",
                        error);
                nativeEditorWindowManager =
                        (WindowManager) getSystemService(Context.WINDOW_SERVICE);
                nativeEditorWindowParams.type =
                        WindowManager.LayoutParams.TYPE_APPLICATION_PANEL;
                nativeEditorWindowParams.token =
                        getWindow().getDecorView().getWindowToken();
                nativeEditorWindowManager.addView(textMenuEditor, nativeEditorWindowParams);
            }
            nativeEditorAttached = true;
        } else {
            nativeEditorWindowManager.updateViewLayout(
                    textMenuEditor, nativeEditorWindowParams);
        }
    }

    private void beginNativeEditorFocusing(float touchX, float touchY) {
        android.util.Log.d(
                "GPUI_NATIVE_INPUT",
                "beginFocus attached=" + nativeEditorAttached
                        + " focusable=" + nativeEditorFocusableMode);
        if (!nativeEditorAttached || nativeEditorFocusableMode || textMenuEditor == null) {
            return;
        }
        textMenuEditor.setClickable(true);
        textMenuEditor.setLongClickable(true);
        textMenuEditor.setFocusable(true);
        textMenuEditor.setFocusableInTouchMode(true);
        textMenuEditor.setTextIsSelectable(true);
        textMenuEditor.setCursorVisible(true);
        textMenuEditor.setMovementMethod(ArrowKeyMovementMethod.getInstance());
        ensureNativeEditorWindow(
                editorLeft, editorTop, editorWidth, editorHeight, true);
        textMenuEditor.postOnAnimation(() -> {
            if (!nativeEditorAttached) {
                android.util.Log.d("GPUI_NATIVE_INPUT", "beginFocus posted: detached");
                return;
            }
            android.util.Log.d("GPUI_NATIVE_INPUT", "beginFocus posted: request");
            textMenuEditor.requestFocus();
            textMenuEditor.requestFocusFromTouch();
            int offset = textMenuEditor.getOffsetForPosition(touchX, touchY);
            int length = textMenuEditor.getText().length();
            offset = Math.max(0, Math.min(offset, length));
            textMenuEditor.setSelection(offset);
            textMenuEditor.revealSelection(offset);
            showImeForView(textMenuEditor, true);
        });
    }

    /** Called by Rust; hides the native editor and restores the IME shadow. */
    public void gpuiHideNativeEditor() {
        runOnUiThreadBlocking(() -> hideTextMenuEditor(true));
    }

    /**
     * Return the current editor contents for a synchronous commit. Pending
     * mirror callbacks are frame-batched; reading the live EditText here
     * prevents the last typed characters from being lost when the user taps
     * outside before the next frame.
     */
    public String gpuiGetNativeEditorText() {
        // Called from the native GPUI thread. Use the volatile UI-thread
        // snapshot instead of touching View internals cross-thread. Retain it
        // after hide too: closing the keyboard or ending ActionMode can hide
        // the editor before the deferred focus commit runs.
        return nativeEditorSnapshot;
    }

    /** Synchronously flush a pending per-frame snapshot before a commit. */
    public void gpuiFlushNativeEditorText() {
        runOnUiThread(() -> {
            if (textMenuEditor == null) {
                return;
            }
            nativeEditorSnapshot = textMenuEditor.getText().toString();
            textMenuEditor.flushMirrorTextNotify();
        });
    }

    private void hideTextMenuEditor() {
        hideTextMenuEditor(false);
    }

    /**
     * End the native editor's focus/IME session without removing it from the
     * page. The persistent EditText remains the visible renderer while the
     * keyboard is closed or the user taps outside the field.
     */
    private void blurTextMenuEditor() {
        blurTextMenuEditor(false);
    }

    private void blurTextMenuEditor(boolean discardPendingText) {
        android.util.Log.d(
                "GPUI_NATIVE_INPUT",
                "blur attached=" + nativeEditorAttached
                        + " discard=" + discardPendingText,
                new RuntimeException("blur trace"));
        if (textMenuEditor == null || !nativeEditorAttached) {
            disableInputViewImeFocus();
            return;
        }
        lastMirrorSelectionStart = -1;
        lastMirrorSelectionEnd = -1;
        if (discardPendingText) {
            // GPUI has already snapshotted nativeEditorSnapshot on outside
            // MouseDown. A queued callback here could arrive after Clear's
            // click handler and restore the previous text state.
            textMenuEditor.cancelPendingMirrorTextNotify();
        } else {
            textMenuEditor.flushMirrorTextNotify();
        }
        textMenuEditor.flushLineRangesNotify();
        textMenuEditor.clearFocus();
        textMenuEditor.restoreNativeSelectionHandles();
        textMenuEditor.setCursorVisible(false);
        textMenuEditor.setTextIsSelectable(false);
        nativeEditorFocusableMode = false;
        nativeOnNativeEditorWindowFocusChanged(false);
        ensureNativeEditorWindow(
                editorLeft, editorTop, editorWidth, editorHeight, false);
        disableInputViewImeFocus();
    }

    private void hideTextMenuEditor(boolean notifyNative) {
        android.util.Log.d(
                "GPUI_NATIVE_INPUT",
                "hide attached=" + nativeEditorAttached
                        + " notify=" + notifyNative,
                new RuntimeException("hide trace"));
        if (textMenuEditor == null || !nativeEditorAttached) {
            disableInputViewImeFocus();
            if (notifyNative) {
                nativeOnNativeEditorHidden();
            }
            return;
        }
        lastMirrorSelectionStart = -1;
        lastMirrorSelectionEnd = -1;
        textMenuEditor.flushMirrorTextNotify();
        textMenuEditor.flushLineRangesNotify();
        textMenuEditor.clearFocus();
        textMenuEditor.setClickable(false);
        textMenuEditor.setLongClickable(false);
        textMenuEditor.setTextIsSelectable(false);
        textMenuEditor.setCursorVisible(false);
        textMenuEditor.setFocusable(false);
        textMenuEditor.setFocusableInTouchMode(false);
        textMenuEditor.setVisibility(View.GONE);
        if (nativeEditorWindowManager != null && nativeEditorAttached) {
            nativeEditorWindowManager.removeView(textMenuEditor);
        }
        nativeEditorAttached = false;
        nativeEditorFocusableMode = false;
        nativeOnNativeEditorWindowFocusChanged(false);
        textMenuEditor.restoreNativeSelectionHandles();
        textMenuEditor.visualScrollTop = 0;
        textMenuEditor.resetNotifiedScroll();
        editorClipBounds = null;
        textMenuEditor.setClipBounds(null);
        editorContentWidth = -1;
        textMenuEditor.setText("", EditText.BufferType.EDITABLE);
        disableInputViewImeFocus();
        if (notifyNative) {
            nativeOnNativeEditorHidden();
        }
    }

    private void disableInputViewImeFocus() {
        if (inputView != null) {
            inputView.setVisibility(View.VISIBLE);
            inputView.setFocusable(false);
            inputView.setFocusableInTouchMode(false);
            inputView.clearFocus();
        }
    }

    /** Called by Rust; stores text into the system clipboard. */
    public void gpuiSetClipboardText(String text) {
        ClipboardManager manager =
                (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        if (manager != null && text != null && !text.isEmpty()) {
            manager.setPrimaryClip(ClipData.newPlainText("text", text));
        }
    }

    /** Display rotation as {@code Surface.ROTATION_*}; used for cutout compensation. */
    public int gpuiDisplayRotation() {
        return getWindowManager().getDefaultDisplay().getRotation();
    }

    private void pasteClipboard() {
        ClipboardManager manager =
                (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        if (manager == null || !manager.hasPrimaryClip()
                || manager.getPrimaryClip() == null
                || manager.getPrimaryClip().getItemCount() == 0) {
            return;
        }
        CharSequence text = manager.getPrimaryClip().getItemAt(0).getText();
        if (text != null && text.length() > 0) {
            nativeCommitText(text.toString());
        }
    }

    /** Clears IME-owned context when focus moves to a different GPUI input. */
    public void gpuiResetInputShadow() {
        runOnUiThread(() -> {
            if (inputView != null) {
                inputView.resetEditable();
            }
        });
    }

    /** Called by Rust after an AccessKit tree update. */
    public void gpuiAccessibilityChanged() {
        if (!accessibilityChangeScheduled.compareAndSet(false, true)) {
            return;
        }
        runOnUiThread(() -> {
            if (inputView == null) {
                accessibilityChangeScheduled.set(false);
                return;
            }
            inputView.postOnAnimation(() -> {
                accessibilityChangeScheduled.set(false);
                inputView.accessibilityProvider.invalidateSnapshot();
                inputView.sendAccessibilityEvent(
                        AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
            });
        });
    }

    /**
     * Ask Android to open a local file with the user's system handler.
     *
     * <p>The GPUI platform thread is not guaranteed to be the Android UI
     * thread, so the entire operation is marshalled through
     * {@link #runOnUiThread(Runnable)}. The provider gives the receiving app a
     * read-only, temporary content URI; a {@code file://} URI is rejected by
     * Android 7 and later.</p>
     */
    public void gpuiOpenWithSystem(String path) {
        if (path == null || path.isEmpty()) {
            return;
        }

        runOnUiThread(() -> {
            File file = new File(path);
            if (!file.isFile()) {
                return;
            }

            try {
                Uri uri = GpuiFileProvider.registerFile(this, file);
                String mimeType = getContentResolver().getType(uri);
                if (mimeType == null) {
                    mimeType = "application/octet-stream";
                }

                Intent intent = new Intent(Intent.ACTION_VIEW)
                        .setDataAndType(uri, mimeType)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                startActivity(intent);
            } catch (ActivityNotFoundException | IOException | SecurityException e) {
                // No compatible handler is a normal user/device condition.
                // Do not include the local path in logs.
                android.util.Log.w("GpuiActivity", "Unable to open file with a system handler", e);
            }
        });
    }

    /** Encrypt and persist a credential using a non-exportable AES-GCM key. */
    public void gpuiWriteCredential(String alias, byte[] secret) throws Exception {
        SecretKey key = getOrCreateKey(alias);
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, key);
        String encoded = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP)
                + ":"
                + Base64.encodeToString(cipher.doFinal(secret), Base64.NO_WRAP);
        getSharedPreferences(CREDENTIAL_PREFS, MODE_PRIVATE)
                .edit()
                .putString(alias, encoded)
                .apply();
    }

    /** Decrypt a credential, or return null when no value exists. */
    public byte[] gpuiReadCredential(String alias) throws Exception {
        String encoded = getSharedPreferences(CREDENTIAL_PREFS, MODE_PRIVATE)
                .getString(alias, null);
        if (encoded == null) {
            return null;
        }
        String[] parts = encoded.split(":", 2);
        if (parts.length != 2) {
            throw new IllegalStateException("Malformed GPUI credential");
        }
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        SecretKey key = (SecretKey) store.getKey(alias, null);
        if (key == null) {
            return null;
        }
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(
                Cipher.DECRYPT_MODE,
                key,
                new GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)));
        return cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP));
    }

    /** Delete both the encrypted payload and its AndroidKeyStore key. */
    public void gpuiDeleteCredential(String alias) throws Exception {
        getSharedPreferences(CREDENTIAL_PREFS, MODE_PRIVATE).edit().remove(alias).apply();
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        if (store.containsAlias(alias)) {
            store.deleteEntry(alias);
        }
    }

    private SecretKey getOrCreateKey(String alias) throws Exception {
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        SecretKey existing = (SecretKey) store.getKey(alias, null);
        if (existing != null) {
            return existing;
        }
        KeyGenerator generator =
                KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        generator.init(new KeyGenParameterSpec.Builder(
                        alias,
                        KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build());
        return generator.generateKey();
    }

    // ---------------------------------------------------------------------
    // Native Cookie dialog panel
    //
    // Renders the whole Cookie dialog — card, title, hint, editor and action
    // buttons — inside a single application window added directly through
    // WindowManager. Card and editor share one surface, so they can never
    // drift apart while the keyboard opens or closes. The window has no
    // Window.Callback, so the framework refuses to start a text ActionMode
    // there; CookieCardLayout intercepts the editor's action-mode request and
    // supplies a floating toolbar for cut/copy/paste/select-all instead while
    // the native selection handles come from the EditText itself.
    // ---------------------------------------------------------------------

    private LinearLayout nativeCookiePanel;
    private EditText nativeCookieEditor;
    private TextView nativeCookieTitleView;
    private TextView nativeCookieHintView;
    private TextView nativeCookieErrorView;
    private LinearLayout nativeCookieButtonRow;
    private boolean nativeCookieCompactLayout;
    private final ExactLineHeightSpan cookieEditorLineHeight = new ExactLineHeightSpan();
    private TextView nativeCookieCancelView;
    private TextView nativeCookieSaveView;
    private WindowManager.LayoutParams nativeCookiePanelParams;
    private WindowManager nativeCookieWindowManager;
    /** Rust wants the panel visible; it survives Activity stop/resume. */
    private boolean nativeCookiePanelActive;
    private boolean nativeCookiePanelAttached;
    private int nativeCookiePanelWidthPx;
    private int nativeCookiePanelImeBottom;
    private int nativeCookieActivityIme;
    private int nativeCookieBarsTop;
    private int nativeCookieBarsLeft;
    private int nativeCookieBarsRight;
    private int nativeCookieLastLayoutY = Integer.MIN_VALUE;
    private int nativeCookieLastLayoutH = Integer.MIN_VALUE;
    private int nativeCookieLastLoggedIme = Integer.MIN_VALUE;
    private int nativeCookieCurrentY = Integer.MIN_VALUE;
    private int nativeCookiePreviousIme = -1;
    /** Back callback registered on the panel window (API 33+). */
    private Object nativeCookieBackCallback;
    private ValueAnimator nativeCookieMoveAnimator;
    private final View.OnLayoutChangeListener nativeCookiePanelLayoutListener =
            (view, left, top, right, bottom, oldLeft, oldTop, oldRight, oldBottom) -> {
                if (bottom - top != oldBottom - oldTop) {
                    layoutNativeCookiePanel();
                }
            };
    private ActionMode nativeCookieActionMode;
    // Creating the floating ActionMode on this standalone panel window makes
    // the vendor TextView collapse a fresh word selection to a caret at the
    // selection end before the toolbar asks for its content rect. Remember the
    // range the editor had when the mode was requested and put it back while
    // the mode is being prepared.
    private int nativeCookieExpectedSelStart = -1;
    private int nativeCookieExpectedSelEnd = -1;
    private long nativeCookieExpectedSelAtMs;
    private static final long COOKIE_SELECTION_REASSERT_MS = 1000;
    private static final String COOKIE_FORMAT_ERROR = "Cookie 格式不正确";
    private static final int COOKIE_ERROR_COLOR = 0xFFE5484D;
    private float nativeCookieDensity = 1f;
    private int nativeCookieCardColor;
    private int nativeCookieFieldColor;
    private int nativeCookieFieldBorder;
    private int nativeCookieFieldFocus;

    /** Called by Rust; shows the Cookie dialog rendered entirely natively. */
    public void gpuiShowNativeCookiePanel(
            String text,
            float widthLogical,
            String title,
            String hint,
            String placeholder,
            String cancelLabel,
            String saveLabel,
            int cardBg,
            int textColor,
            int mutedColor,
            int fieldBg,
            int fieldBorder,
            int fieldFocus,
            int primary,
            int primaryText,
            int cancelBg,
            int cancelBorder,
            int cancelText) {
        runOnUiThread(() -> {
            nativeCookieDensity = getResources().getDisplayMetrics().density;
            nativeCookieCardColor = gpuiColorToArgb(cardBg);
            nativeCookieFieldColor = gpuiColorToArgb(fieldBg);
            nativeCookieFieldBorder = gpuiColorToArgb(fieldBorder);
            nativeCookieFieldFocus = gpuiColorToArgb(fieldFocus);
            nativeCookiePanelWidthPx = Math.max(
                    1, Math.round(widthLogical * nativeCookieDensity));
            buildNativeCookiePanel(
                    title,
                    hint,
                    placeholder,
                    cancelLabel,
                    saveLabel,
                    textColor,
                    mutedColor,
                    primary,
                    primaryText,
                    cancelBg,
                    cancelBorder,
                    cancelText);
            nativeCookieEditor.setText(text);
            nativeCookieEditor.setSelection(nativeCookieEditor.getText().length());
            applyCookieEditorLineHeight();
            nativeCookiePanelActive = true;
            // Opening the dialog must not raise the keyboard; the editor
            // focuses (and moves the card above the IME) only when tapped.
            attachNativeCookiePanel(false);
        });
    }

    /** Called by Rust; removes the Cookie panel window. */
    public void gpuiHideNativeCookiePanel() {
        // Synchronous: the dialog frame is already gone, and a posted removal
        // would leave the card floating over the page for a frame or two.
        runOnUiThreadBlocking(() -> {
            nativeCookiePanelActive = false;
            detachNativeCookiePanel();
        });
    }

    private void buildNativeCookiePanel(
            String title,
            String hint,
            String placeholder,
            String cancelLabel,
            String saveLabel,
            int textColor,
            int mutedColor,
            int primary,
            int primaryText,
            int cancelBg,
            int cancelBorder,
            int cancelText) {
        detachNativeCookiePanel();
        float density = nativeCookieDensity;
        int pad = Math.round(24 * density);

        CookieCardLayout card = new CookieCardLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setBackground(roundedRect(nativeCookieCardColor, 24 * density, 0, 0));
        card.setPadding(pad, pad, pad, pad);
        // Selection handles live in this layout and may extend past the card
        // edge, so they must not be clipped.
        card.setClipChildren(false);
        card.setClipToPadding(false);

        TextView titleView = new TextView(this);
        titleView.setText(title);
        titleView.setTextSize(TypedValue.COMPLEX_UNIT_PX, 22 * density);
        titleView.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        titleView.setTextColor(gpuiColorToArgb(textColor));
        titleView.setIncludeFontPadding(false);
        card.addView(titleView, panelWrap());

        TextView hintView = new TextView(this);
        hintView.setText(hint);
        hintView.setTextSize(TypedValue.COMPLEX_UNIT_PX, 13 * density);
        hintView.setTextColor(gpuiColorToArgb(mutedColor));
        hintView.setIncludeFontPadding(false);
        hintView.setLineSpacing(Math.round(4 * density), 1.0f);
        LinearLayout.LayoutParams hintParams = panelWrap();
        hintParams.topMargin = Math.round(16 * density);
        card.addView(hintView, hintParams);

        EditText editor = new EditText(this);
        editor.setTextSize(TypedValue.COMPLEX_UNIT_PX, 16 * density);
        editor.setTextColor(gpuiColorToArgb(textColor));
        editor.setHintTextColor(gpuiColorToArgb(mutedColor));
        editor.setHint(placeholder);
        editor.setIncludeFontPadding(false);
        // The vendor font's fallback metrics give every line a huge box even
        // with zero line spacing; pin the line boxes to a compact height like
        // the home input does.
        cookieEditorLineHeight.setLineHeightPx(Math.round(21 * density));
        editor.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {}

            @Override
            public void afterTextChanged(Editable s) {
                if (nativeCookieErrorView != null
                        && nativeCookieErrorView.getVisibility() != View.GONE) {
                    nativeCookieErrorView.setVisibility(View.GONE);
                    applyCookieEditorBackground();
                    layoutNativeCookiePanel();
                }
                applyCookieEditorLineHeight();
            }
        });
        editor.setLineSpacing(0, 1.0f);
        editor.setGravity(Gravity.TOP | Gravity.START);
        editor.setPadding(
                Math.round(12 * density),
                Math.round(12 * density),
                Math.round(12 * density),
                Math.round(12 * density));
        editor.setBackground(
                roundedRect(nativeCookieFieldColor, 6 * density, 1, nativeCookieFieldBorder));
        // Plain multi-line text: a password variation would switch MIUI to
        // its secure keyboard and disable selection/copy/paste.
        editor.setInputType(
                InputType.TYPE_CLASS_TEXT
                        | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                        | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        editor.setImeOptions(
                EditorInfo.IME_ACTION_NONE
                        | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                        | EditorInfo.IME_FLAG_NO_FULLSCREEN);
        editor.setVerticalScrollBarEnabled(true);
        editor.setHorizontalScrollBarEnabled(false);
        editor.setSelectAllOnFocus(false);
        editor.setFocusable(true);
        editor.setFocusableInTouchMode(true);
        editor.setTextIsSelectable(true);
        // Vendor textual "smart selection" classifies the whole cookie as a
        // URL and rewrites a double-tap word selection to the full text,
        // which then collapses to a caret at the end. NO_OP keeps the stock
        // word selection that double tap produces.
        editor.setTextClassifier(TextClassifier.NO_OP);
        // Editor.prepareCursorControllers() runs before the first layout, when
        // mLayout is still null, and disables the selection controller for
        // good. TextView never re-runs it after layout, so long press reports
        // "TextView does not support text selection" on this standalone
        // window. Toggling cursor visibility makes TextView run it again now
        // that a layout exists.
        editor.addOnLayoutChangeListener(
                (view, l, t, r, b, oldL, oldT, oldR, oldB) ->
                        refreshNativeCookieSelectionControllers());
        editor.setOnTouchListener((view, event) -> {
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                clearCookieTextSelectionCapture();
                Log.d("TextTest", "EditText down x=" + event.getX() + " y=" + event.getY()
                        + " selection=" + editor.getSelectionStart()
                        + ".." + editor.getSelectionEnd());
            } else if (event.getActionMasked() == MotionEvent.ACTION_UP) {
                Log.d("TextTest", "EditText up x=" + event.getX() + " y=" + event.getY()
                        + " selection=" + editor.getSelectionStart()
                        + ".." + editor.getSelectionEnd());
            }
            return false;
        });
        editor.setOnLongClickListener(view -> {
            Log.d("TextTest", "EditText long click");
            Log.d(
                    "TextTest",
                    "focus=" + view.hasFocus()
                            + " windowFocus=" + view.hasWindowFocus()
                            + " selection=" + editor.getSelectionStart()
                            + ".." + editor.getSelectionEnd()
                            + " layout=" + (editor.getLayout() != null)
                            + " attached=" + editor.isAttachedToWindow()
                            + " shown=" + editor.isShown()
                            + " root=" + describeWindowParams(editor));
            return false;
        });
        editor.setOnFocusChangeListener((view, hasFocus) -> applyCookieEditorBackground());
        LinearLayout.LayoutParams editorParams = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, Math.round(146 * density));
        editorParams.topMargin = Math.round(8 * density);
        card.addView(editor, editorParams);

        TextView errorView = new TextView(this);
        errorView.setTextSize(TypedValue.COMPLEX_UNIT_PX, 12 * density);
        errorView.setTextColor(COOKIE_ERROR_COLOR);
        errorView.setIncludeFontPadding(false);
        errorView.setLineSpacing(0, 1.0f);
        errorView.setVisibility(View.GONE);
        LinearLayout.LayoutParams errorParams = panelWrap();
        errorParams.topMargin = Math.round(8 * density);
        card.addView(errorView, errorParams);

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.END | Gravity.CENTER_VERTICAL);
        LinearLayout.LayoutParams rowParams = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT);
        rowParams.topMargin = Math.round(16 * density);
        card.addView(row, rowParams);

        TextView cancel = panelButton(
                cancelLabel, gpuiColorToArgb(cancelText), gpuiColorToArgb(cancelBg),
                gpuiColorToArgb(cancelBorder));
        LinearLayout.LayoutParams cancelParams = panelWrap();
        cancelParams.rightMargin = Math.round(12 * density);
        row.addView(cancel, cancelParams);

        TextView save = panelButton(
                saveLabel, gpuiColorToArgb(primaryText), gpuiColorToArgb(primary),
                gpuiColorToArgb(primary));
        // Same horizontal brand gradient as the app's primary buttons
        // (assets/gradient/button-background-rounded-8.svg).
        save.setBackground(gradientButton(8 * density, gpuiColorToArgb(primary)));
        row.addView(save, panelWrap());

        cancel.setOnClickListener(view -> nativeOnNativeCookieCancel());
        save.setOnClickListener(view -> {
            String text = nativeCookieEditor.getText().toString();
            String error = validateCookieHeader(text);
            if (error != null) {
                errorView.setText(error);
                errorView.setVisibility(View.VISIBLE);
                applyCookieEditorBackground();
                layoutNativeCookiePanel();
                return;
            }
            errorView.setVisibility(View.GONE);
            applyCookieEditorBackground();
            nativeOnNativeCookieSave(text);
        });

        nativeCookiePanel = card;
        nativeCookieEditor = editor;
        nativeCookieTitleView = titleView;
        nativeCookieHintView = hintView;
        nativeCookieErrorView = errorView;
        nativeCookieButtonRow = row;
        nativeCookieCancelView = cancel;
        nativeCookieSaveView = save;
        nativeCookieCurrentY = Integer.MIN_VALUE;
        nativeCookiePreviousIme = -1;
        nativeCookieCompactLayout = false;
    }

    /**
     * Mirrors the card width Rust computes for the Cookie dialog
     * (min(viewport-48, 80%, 480) logical px) against the current display.
     * Needed after rotation, because the panel is only re-specified when the
     * dialog is opened.
     */
    private void recomputeNativeCookiePanelWidth() {
        int screenWidth;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            screenWidth = getWindowManager().getMaximumWindowMetrics().getBounds().width();
        } else {
            screenWidth = getResources().getDisplayMetrics().widthPixels;
        }
        int width = Math.min(
                screenWidth - Math.round(48 * nativeCookieDensity),
                Math.min(
                        Math.round(screenWidth * 0.8f),
                        Math.round(480 * nativeCookieDensity)));
        nativeCookiePanelWidthPx = Math.max(1, width);
    }

    /**
     * Cookie header must be `name=value; name2=value2`. Names follow RFC 6265
     * tokens, so pasted URLs (which contain ':' and '/') and free text are
     * rejected. An empty value clears the stored cookie.
     *
     * @return an error message, or {@code null} when the input is acceptable
     */
    private static String validateCookieHeader(String raw) {
        String value = raw == null ? "" : raw.trim();
        if (value.isEmpty()) {
            return null;
        }
        for (int i = 0; i < value.length(); i++) {
            if (Character.isISOControl(value.charAt(i))) {
                return COOKIE_FORMAT_ERROR;
            }
        }
        int pairs = 0;
        for (String segment : value.split(";", -1)) {
            String part = segment.trim();
            if (part.isEmpty()) {
                continue;
            }
            int equals = part.indexOf('=');
            if (equals <= 0) {
                return COOKIE_FORMAT_ERROR;
            }
            String name = part.substring(0, equals).trim();
            if (!isCookieName(name)) {
                return COOKIE_FORMAT_ERROR;
            }
            pairs++;
        }
        return pairs > 0 ? null : COOKIE_FORMAT_ERROR;
    }

    private static boolean isCookieName(String name) {
        if (name.isEmpty()) {
            return false;
        }
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            boolean allowed = (c >= 'a' && c <= 'z')
                    || (c >= 'A' && c <= 'Z')
                    || (c >= '0' && c <= '9')
                    || "!#$%&'*+-.^_`|~".indexOf(c) >= 0;
            if (!allowed) {
                return false;
            }
        }
        return true;
    }

    private void applyCookieEditorBackground() {
        if (nativeCookieEditor == null) {
            return;
        }
        // Compact mode shows this border as the only chrome, so keep the
        // focused outline visibly thick at that size.
        int borderPx = nativeCookieCompactLayout
                ? Math.max(1, Math.round(1.5f * nativeCookieDensity))
                : 1;
        boolean showError = nativeCookieErrorView != null
                && nativeCookieErrorView.getVisibility() == View.VISIBLE;
        int borderColor;
        if (showError) {
            borderColor = COOKIE_ERROR_COLOR;
        } else if (nativeCookieEditor.hasFocus()) {
            borderColor = nativeCookieFieldFocus;
        } else {
            borderColor = nativeCookieFieldBorder;
        }
        nativeCookieEditor.setBackground(roundedRect(
                nativeCookieFieldColor,
                6 * nativeCookieDensity,
                borderPx,
                borderColor));
    }

    private void applyCookieEditorLineHeight() {
        if (nativeCookieEditor == null) {
            return;
        }
        Editable text = nativeCookieEditor.getEditableText();
        if (text == null) {
            return;
        }
        text.removeSpan(cookieEditorLineHeight);
        if (text.length() > 0) {
            text.setSpan(
                    cookieEditorLineHeight,
                    0,
                    text.length(),
                    Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        }
    }

    private LinearLayout.LayoutParams panelWrap() {
        return new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT);
    }

    private String describeWindowParams(View view) {
        View root = view.getRootView();
        ViewGroup.LayoutParams params = root.getLayoutParams();
        if (params instanceof WindowManager.LayoutParams) {
            WindowManager.LayoutParams windowParams = (WindowManager.LayoutParams) params;
            return windowParams.type
                    + "/token=" + (windowParams.token != null)
                    + "/flags=" + Integer.toHexString(windowParams.flags);
        }
        return String.valueOf(params);
    }

    private GradientDrawable roundedRect(int color, float radius, int strokeWidth, int strokeColor) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setColor(color);
        drawable.setCornerRadius(radius);
        if (strokeWidth > 0) {
            drawable.setStroke(strokeWidth, strokeColor);
        }
        return drawable;
    }

    private GradientDrawable gradientButton(float radius, int borderColor) {
        GradientDrawable drawable = new GradientDrawable(
                GradientDrawable.Orientation.LEFT_RIGHT,
                new int[] { 0xFF0277F9, 0xFF5D47E3 });
        drawable.setCornerRadius(radius);
        drawable.setStroke(1, borderColor);
        return drawable;
    }

    private TextView panelButton(String label, int textColor, int bgColor, int borderColor) {
        float density = nativeCookieDensity;
        TextView view = new TextView(this);
        view.setText(label);
        view.setTextSize(TypedValue.COMPLEX_UNIT_PX, 14 * density);
        view.setTextColor(textColor);
        view.setGravity(Gravity.CENTER);
        view.setIncludeFontPadding(false);
        view.setPadding(Math.round(20 * density), 0, Math.round(20 * density), 0);
        view.setMinWidth(Math.round(72 * density));
        view.setMinHeight(Math.round(34 * density));
        view.setBackground(roundedRect(bgColor, 8 * density, 1, borderColor));
        view.setClickable(true);
        view.setFocusable(false);
        view.setSoundEffectsEnabled(true);
        return view;
    }

    private void attachNativeCookiePanel(boolean takeFocus) {
        if (nativeCookiePanel == null) {
            return;
        }
        if (nativeCookieWindowManager == null) {
            nativeCookieWindowManager = getWindow().getWindowManager();
        }
        if (nativeCookieWindowManager == null) {
            return;
        }
        if (!nativeCookiePanelAttached) {
            if (nativeCookiePanelParams == null) {
                int flags = WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                        | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED
                        | WindowManager.LayoutParams.FLAG_SPLIT_TOUCH;
                nativeCookiePanelParams = new WindowManager.LayoutParams(
                        nativeCookiePanelWidthPx,
                        WindowManager.LayoutParams.WRAP_CONTENT,
                        WindowManager.LayoutParams.TYPE_APPLICATION,
                        flags,
                        android.graphics.PixelFormat.TRANSLUCENT);
                nativeCookiePanelParams.gravity = Gravity.TOP | Gravity.START;
                nativeCookiePanelParams.softInputMode =
                        WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING;
                nativeCookiePanelParams.setTitle("ReelsCookieDialog");
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    nativeCookiePanelParams.setFitInsetsTypes(0);
                }
            }
            nativeCookieCurrentY = Integer.MIN_VALUE;
            nativeCookiePreviousIme = -1;
            nativeCookieActivityIme = imeBottomPixels;
            nativeCookieBarsTop = lastInsetsTop;
            layoutNativeCookiePanel();
            try {
                nativeCookieWindowManager.addView(nativeCookiePanel, nativeCookiePanelParams);
            } catch (WindowManager.BadTokenException error) {
                if (nativeCookiePanelParams.type
                        != WindowManager.LayoutParams.TYPE_APPLICATION) {
                    throw error;
                }
                android.util.Log.w(
                        "GpuiActivity",
                        "Application cookie panel rejected; falling back to panel",
                        error);
                nativeCookieWindowManager =
                        (WindowManager) getSystemService(Context.WINDOW_SERVICE);
                nativeCookiePanelParams.type = WindowManager.LayoutParams.TYPE_APPLICATION_PANEL;
                nativeCookiePanelParams.token =
                        getWindow().getDecorView().getWindowToken();
                nativeCookieWindowManager.addView(nativeCookiePanel, nativeCookiePanelParams);
            }
            nativeCookiePanelAttached = true;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                registerNativeCookieBackCallback();
            }
            nativeCookiePanel.requestApplyInsets();
            nativeCookiePanel.removeOnLayoutChangeListener(nativeCookiePanelLayoutListener);
            nativeCookiePanel.addOnLayoutChangeListener(nativeCookiePanelLayoutListener);
        }
        if (takeFocus) {
            nativeCookiePanel.post(this::focusNativeCookiePanel);
        }
    }

    private void refreshNativeCookieSelectionControllers() {
        if (nativeCookieEditor == null || nativeCookieEditor.getLayout() == null) {
            return;
        }
        boolean cursorVisible = nativeCookieEditor.isCursorVisible();
        nativeCookieEditor.setCursorVisible(!cursorVisible);
        nativeCookieEditor.setCursorVisible(cursorVisible);
        Log.d(
                "TextTest",
                "selection controllers refreshed layout="
                        + (nativeCookieEditor.getLayout() != null));
    }

    private void focusNativeCookiePanel() {
        if (!nativeCookiePanelAttached || nativeCookieEditor == null) {
            return;
        }
        boolean focused = nativeCookieEditor.requestFocus();
        nativeCookieEditor.requestFocusFromTouch();
        Log.d(
                "TextTest",
                "focus requested=" + focused
                        + " hasFocus=" + nativeCookieEditor.hasFocus()
                        + " windowFocus=" + nativeCookieEditor.hasWindowFocus()
                        + " shown=" + nativeCookieEditor.isShown()
                        + " root=" + describeWindowParams(nativeCookieEditor));
        requestImeForView(nativeCookieEditor, true);
    }

    /**
     * Back pressed while the Cookie panel is up.
     *
     * <p>The panel is its own WindowManager window, so the Activity's back
     * dispatcher does not see the key while the panel holds focus. With the
     * keyboard open, back closes the keyboard and drops the editor focus but
     * leaves the card; without it, back closes the dialog like Cancel.</p>
     */
    private void handleNativeCookieBack() {
        if (!nativeCookiePanelActive || !nativeCookiePanelAttached) {
            nativeOnBackPressed();
            return;
        }
        // Same threshold as the layout: a floating vendor keyboard reports a
        // one-pixel IME inset and counts as closed.
        boolean keyboardUp = nativeCookieActivityIme >= Math.round(60 * nativeCookieDensity);
        if (keyboardUp) {
            InputMethodManager manager =
                    (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (manager != null && nativeCookieEditor != null) {
                manager.hideSoftInputFromWindow(nativeCookieEditor.getWindowToken(), 0);
            }
            if (nativeCookieEditor != null && nativeCookieEditor.hasFocus()) {
                nativeCookieEditor.clearFocus();
                applyCookieEditorBackground();
            }
            return;
        }
        nativeOnNativeCookieCancel();
    }

    @android.annotation.TargetApi(Build.VERSION_CODES.TIRAMISU)
    private void registerNativeCookieBackCallback() {
        if (nativeCookieBackCallback != null || nativeCookiePanel == null) {
            return;
        }
        android.window.OnBackInvokedDispatcher dispatcher =
                nativeCookiePanel.findOnBackInvokedDispatcher();
        if (dispatcher == null) {
            return;
        }
        android.window.OnBackInvokedCallback callback = this::handleNativeCookieBack;
        try {
            dispatcher.registerOnBackInvokedCallback(
                    android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            nativeCookieBackCallback = callback;
        } catch (IllegalArgumentException | IllegalStateException error) {
            // Fall back to the panel's key-event handling.
            Log.w("GPUICookiePanel", "back callback registration failed", error);
        }
    }

    private void unregisterNativeCookieBackCallback() {
        if (nativeCookieBackCallback == null) {
            return;
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
                && nativeCookiePanel != null) {
            android.window.OnBackInvokedDispatcher dispatcher =
                    nativeCookiePanel.findOnBackInvokedDispatcher();
            if (dispatcher != null) {
                try {
                    dispatcher.unregisterOnBackInvokedCallback(
                            (android.window.OnBackInvokedCallback) nativeCookieBackCallback);
                } catch (IllegalArgumentException ignored) {
                    // Already unregistered together with the window.
                }
            }
        }
        nativeCookieBackCallback = null;
    }

    private void detachNativeCookiePanel() {
        if (nativeCookiePanel == null || !nativeCookiePanelAttached) {
            return;
        }
        if (nativeCookieActionMode != null) {
            nativeCookieActionMode.finish();
            nativeCookieActionMode = null;
        }
        if (nativeCookieMoveAnimator != null) {
            nativeCookieMoveAnimator.cancel();
            nativeCookieMoveAnimator = null;
        }
        InputMethodManager manager =
                (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        if (manager != null) {
            manager.hideSoftInputFromWindow(nativeCookiePanel.getWindowToken(), 0);
        }
        nativeCookieEditor.clearFocus();
        unregisterNativeCookieBackCallback();
        if (nativeCookieWindowManager != null) {
            try {
                nativeCookieWindowManager.removeView(nativeCookiePanel);
            } catch (IllegalArgumentException ignored) {
                // The window was already detached by the system.
            }
        }
        nativeCookiePanelAttached = false;
        nativeCookieCurrentY = Integer.MIN_VALUE;
    }

    /**
     * Centre the card in the visible area (the whole screen without the
     * keyboard, or the screen minus the keyboard when it is open) and move
     * the single panel window there, so card and editor can never move
     * relative to each other.
     *
     * <p>Only the Activity window's IME inset is used. The panel window's own
     * insets depend on where the window currently sits, and reading them here
     * fed the layout back into itself — the endless bounce seen in landscape.
     * </p>
     */
    private void layoutNativeCookiePanel() {
        if (nativeCookiePanel == null || nativeCookiePanelParams == null) {
            return;
        }
        int width = nativeCookiePanelWidthPx;

        int screenWidth;
        int screenHeight;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Rect screen = getWindowManager().getMaximumWindowMetrics().getBounds();
            screenWidth = screen.width();
            screenHeight = screen.height();
        } else {
            android.util.DisplayMetrics metrics = getResources().getDisplayMetrics();
            screenWidth = metrics.widthPixels;
            screenHeight = metrics.heightPixels;
        }
        int ime = Math.min(Math.max(0, nativeCookieActivityIme), screenHeight);
        // The vendor's floating keyboard reports a one-pixel IME inset while
        // the window keeps the whole display; a real docked keyboard is far
        // taller. Ignore keyboard heights that cannot be a docked keyboard.
        if (ime < Math.round(60 * nativeCookieDensity)) {
            ime = 0;
        }
        if (ime > 0) {
            // Trust the inset only when the Activity's visible frame actually
            // ends at the keyboard. A floating vendor keyboard (and a stale
            // inset left over from the previous rotation) reports a height
            // while the window still owns the whole display; the card must
            // not move in that case.
            Rect visibleFrame = new Rect();
            getWindow().getDecorView().getWindowVisibleDisplayFrame(visibleFrame);
            if (visibleFrame.bottom > 0
                    && visibleFrame.bottom > screenHeight - ime + 32) {
                ime = 0;
            }
        }
        int contentTop = Math.max(0, nativeCookieBarsTop);
        int spaceAboveKeyboard = ime > 0 ? Math.max(0, screenHeight - ime - contentTop) : 0;
        // Landscape with a docked keyboard leaves too little room for the
        // whole card. Shrink to just the editor filling the space above the
        // keyboard, the way the home input sits there.
        boolean compact = ime > 0
                && screenWidth > screenHeight
                && spaceAboveKeyboard > 0
                && spaceAboveKeyboard < Math.round(320 * nativeCookieDensity);
        int safeLeft = compact ? Math.max(0, nativeCookieBarsLeft) : 0;
        int safeRight = compact ? Math.max(0, nativeCookieBarsRight) : 0;
        int x = compact ? safeLeft : Math.max(0, (screenWidth - width) / 2);
        int panelWidth = compact ? screenWidth - safeLeft - safeRight : width;
        applyNativeCookieCompactMode(compact, spaceAboveKeyboard);

        int height;
        if (compact) {
            height = spaceAboveKeyboard;
        } else {
            nativeCookiePanel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
            // Always measure the natural card height. Reading getHeight() here
            // would feed the previously applied window height (e.g. the
            // compact strip) back into the next layout after rotation.
            height = nativeCookiePanel.getMeasuredHeight();
        }

        int y;
        if (compact) {
            y = contentTop;
        } else if (ime > 0) {
            y = contentTop
                    + Math.max(0, (screenHeight - ime - contentTop - height) / 2);
        } else {
            y = Math.max(0, (screenHeight - height) / 2);
        }

        if (nativeCookieLastLoggedIme != ime
                || nativeCookieLastLayoutY != y
                || nativeCookieLastLayoutH != height) {
            nativeCookieLastLoggedIme = ime;
            nativeCookieLastLayoutY = y;
            nativeCookieLastLayoutH = height;
            android.util.Log.d(
                    "GPUICookiePanel",
                    "ime=" + ime
                            + " activityTop=" + nativeCookieBarsTop
                            + " screen=" + screenWidth + "x" + screenHeight
                            + " panel=" + panelWidth + "x" + height
                            + " compact=" + compact
                            + " at=" + x + "," + y);
        }

        nativeCookiePanelParams.width = panelWidth;
        nativeCookiePanelParams.height = height;
        nativeCookiePanelParams.x = x;
        applyNativeCookiePanelY(y, ime);
    }

    /**
     * Compact state for landscape with a docked keyboard: only the editor
     * stays visible, filling the strip above the keyboard (the home input
     * layout), with title, hint and buttons hidden until the keyboard closes.
     */
    private void applyNativeCookieCompactMode(boolean compact, int spaceAboveKeyboard) {
        if (nativeCookiePanel == null || nativeCookieEditor == null) {
            return;
        }
        boolean modeChanged = compact != nativeCookieCompactLayout;
        nativeCookieCompactLayout = compact;
        int pad = Math.round((compact ? 16 : 24) * nativeCookieDensity);
        if (modeChanged) {
            int visibility = compact ? View.GONE : View.VISIBLE;
            if (nativeCookieTitleView != null) {
                nativeCookieTitleView.setVisibility(visibility);
            }
            if (nativeCookieHintView != null) {
                nativeCookieHintView.setVisibility(visibility);
            }
            if (nativeCookieButtonRow != null) {
                nativeCookieButtonRow.setVisibility(visibility);
            }
            if (nativeCookieErrorView != null && compact) {
                nativeCookieErrorView.setVisibility(View.GONE);
            }
            nativeCookiePanel.setPadding(pad, pad, pad, pad);
            // In compact the bordered editor itself is the visible surface,
            // like the home input; the card behind it stays transparent.
            nativeCookiePanel.setBackground(compact
                    ? null
                    : roundedRect(nativeCookieCardColor, 24 * nativeCookieDensity, 0, 0));
            applyCookieEditorBackground();
        }
        int margin = compact ? 0 : Math.round(8 * nativeCookieDensity);
        int editorHeight = compact
                ? Math.max(1, spaceAboveKeyboard - 2 * pad)
                : Math.round(146 * nativeCookieDensity);
        ViewGroup.LayoutParams params = nativeCookieEditor.getLayoutParams();
        if (params instanceof LinearLayout.LayoutParams) {
            LinearLayout.LayoutParams editorParams = (LinearLayout.LayoutParams) params;
            if (editorParams.height != editorHeight || editorParams.topMargin != margin) {
                editorParams.height = editorHeight;
                editorParams.topMargin = margin;
                nativeCookieEditor.setLayoutParams(editorParams);
            }
        }
    }

    private void applyNativeCookiePanelY(int y, int ime) {
        boolean keyboardToggled = nativeCookiePreviousIme >= 0
                && (nativeCookiePreviousIme == 0) != (ime == 0);
        nativeCookiePreviousIme = ime;
        if (!nativeCookiePanelAttached) {
            nativeCookiePanelParams.y = y;
            nativeCookieCurrentY = y;
            return;
        }
        if (nativeCookieMoveAnimator != null) {
            nativeCookieMoveAnimator.cancel();
            nativeCookieMoveAnimator = null;
        }
        if (!keyboardToggled
                || nativeCookieCurrentY == Integer.MIN_VALUE
                || Math.abs(y - nativeCookieCurrentY) <= 1) {
            setNativeCookiePanelY(y);
            return;
        }
        // The ROM reports the keyboard height in one step (no inset animation
        // frames), so glide the window to the new centre instead of snapping.
        ValueAnimator animator = ValueAnimator.ofInt(nativeCookieCurrentY, y);
        animator.setDuration(200);
        animator.setInterpolator(new DecelerateInterpolator());
        animator.addUpdateListener(
                animation -> setNativeCookiePanelY((int) animation.getAnimatedValue()));
        animator.start();
        nativeCookieMoveAnimator = animator;
    }

    private void setNativeCookiePanelY(int y) {
        if (nativeCookiePanelParams == null) {
            return;
        }
        nativeCookiePanelParams.y = y;
        if (nativeCookiePanelAttached && nativeCookieWindowManager != null) {
            try {
                nativeCookieWindowManager.updateViewLayout(
                        nativeCookiePanel, nativeCookiePanelParams);
            } catch (IllegalArgumentException ignored) {
                // The window was detached mid-animation.
            }
        }
        nativeCookieCurrentY = y;
    }

    /**
     * Card container that answers the editor's ActionMode request.
     *
     * <p>A view hierarchy added through WindowManager has no
     * Window.Callback, and ViewRootImpl returns null for every
     * startActionMode call. Without an ActionMode the EditText never shows
     * its selection handles or edit menu, so this layout supplies one.</p>
     */
    private class CookieCardLayout extends LinearLayout {
        CookieCardLayout(Context context) {
            super(context);
        }

        @Override
        public boolean dispatchKeyEvent(KeyEvent event) {
            // The panel is a standalone window with no Window.Callback, so an
            // unhandled BACK would be dropped here instead of reaching the
            // Activity. Handle it on the panel: collapse the keyboard first,
            // close the dialog only when no keyboard is showing.
            if (event.getKeyCode() == KeyEvent.KEYCODE_BACK) {
                if (event.getAction() == KeyEvent.ACTION_UP && !event.isCanceled()) {
                    handleNativeCookieBack();
                }
                return true;
            }
            return super.dispatchKeyEvent(event);
        }

        @Override
        public ActionMode startActionModeForChild(
                View originalView, ActionMode.Callback callback, int type) {
            Log.d(
                    "GPUICookiePanel",
                    "startActionModeForChild type=" + type
                            + " view=" + originalView.getClass().getSimpleName()
                            + " isEditor=" + (originalView == nativeCookieEditor));
            return startCookieTextActionMode(originalView, callback);
        }

        @Override
        public ActionMode startActionModeForChild(
                View originalView, ActionMode.Callback callback) {
            Log.d(
                    "GPUICookiePanel",
                    "startActionModeForChild legacy view="
                            + originalView.getClass().getSimpleName()
                            + " isEditor=" + (originalView == nativeCookieEditor));
            return startCookieTextActionMode(originalView, callback);
        }
    }

    private ActionMode startCookieTextActionMode(
            View originalView, ActionMode.Callback callback) {
        Log.d(
                "GPUICookiePanel",
                "startCookieTextActionMode isEditor=" + (originalView == nativeCookieEditor)
                        + " attached=" + nativeCookiePanelAttached
                        + " selection=" + (nativeCookieEditor == null
                                ? "null"
                                : nativeCookieEditor.getSelectionStart()
                                        + ".." + nativeCookieEditor.getSelectionEnd()));
        if (originalView != nativeCookieEditor || !nativeCookiePanelAttached) {
            return null;
        }
        captureCookieTextSelection();
        if (nativeCookieActionMode != null) {
            nativeCookieActionMode.finish();
            nativeCookieActionMode = null;
        }
        // The real system ActionMode lives in the Activity window, so the
        // panel window (no Window.Callback of its own) can still hand the
        // editor a floating ActionMode. Starting it through the DecorView
        // with the editor as originating view makes the framework anchor the
        // stock (vendor-customised) toolbar to the selection.
        ActionMode systemMode = null;
        View decor = getWindow().getDecorView();
        if (decor instanceof ViewGroup) {
            systemMode = ((ViewGroup) decor).startActionModeForChild(
                    nativeCookieEditor,
                    wrapCookieTextCallback(callback),
                    ActionMode.TYPE_FLOATING);
        }
        if (systemMode != null) {
            Log.d("GPUICookiePanel", "system floating action mode started");
            nativeCookieActionMode = systemMode;
            return systemMode;
        }
        Log.d("GPUICookiePanel", "system action mode refused; in-window toolbar instead");
        CookieActionMode mode = new CookieActionMode(callback);
        if (!mode.start()) {
            Log.d("GPUICookiePanel", "cookie action mode onCreate returned false");
            return null;
        }
        Log.d("GPUICookiePanel", "cookie action mode started");
        nativeCookieActionMode = mode;
        return mode;
    }

    private void captureCookieTextSelection() {
        nativeCookieExpectedSelStart = -1;
        nativeCookieExpectedSelEnd = -1;
        if (nativeCookieEditor == null) {
            return;
        }
        int start = nativeCookieEditor.getSelectionStart();
        int end = nativeCookieEditor.getSelectionEnd();
        if (start < 0 || end < 0 || start == end) {
            return;
        }
        nativeCookieExpectedSelStart = Math.min(start, end);
        nativeCookieExpectedSelEnd = Math.max(start, end);
        nativeCookieExpectedSelAtMs = SystemClock.uptimeMillis();
        Log.d(
                "GPUICookiePanel",
                "captured selection=" + nativeCookieExpectedSelStart
                        + ".." + nativeCookieExpectedSelEnd);
    }

    private void clearCookieTextSelectionCapture() {
        nativeCookieExpectedSelStart = -1;
        nativeCookieExpectedSelEnd = -1;
    }

    private void reassertCookieTextSelection() {
        if (nativeCookieEditor == null
                || nativeCookieExpectedSelStart < 0
                || nativeCookieExpectedSelEnd < 0) {
            return;
        }
        if (SystemClock.uptimeMillis() - nativeCookieExpectedSelAtMs
                > COOKIE_SELECTION_REASSERT_MS) {
            clearCookieTextSelectionCapture();
            return;
        }
        CharSequence text = nativeCookieEditor.getText();
        if (text == null
                || nativeCookieExpectedSelEnd > text.length()
                || nativeCookieExpectedSelEnd <= nativeCookieExpectedSelStart) {
            clearCookieTextSelectionCapture();
            return;
        }
        int start = nativeCookieEditor.getSelectionStart();
        int end = nativeCookieEditor.getSelectionEnd();
        if (start == nativeCookieExpectedSelStart && end == nativeCookieExpectedSelEnd) {
            return;
        }
        if (start != end) {
            // The selection changed to some other range on purpose (select
            // all, handle drag); leave it alone.
            clearCookieTextSelectionCapture();
            return;
        }
        Selection.setSelection(
                nativeCookieEditor.getEditableText(),
                nativeCookieExpectedSelStart,
                nativeCookieExpectedSelEnd);
        Log.d(
                "GPUICookiePanel",
                "reasserted selection=" + nativeCookieExpectedSelStart
                        + ".." + nativeCookieExpectedSelEnd
                        + " was=" + start + ".." + end);
    }

    private ActionMode.Callback wrapCookieTextCallback(ActionMode.Callback callback) {
        return new ActionMode.Callback2() {
            @Override
            public boolean onCreateActionMode(ActionMode mode, Menu menu) {
                reassertCookieTextSelection();
                return callback.onCreateActionMode(mode, menu);
            }

            @Override
            public boolean onPrepareActionMode(ActionMode mode, Menu menu) {
                reassertCookieTextSelection();
                return callback.onPrepareActionMode(mode, menu);
            }

            @Override
            public boolean onActionItemClicked(ActionMode mode, MenuItem item) {
                return callback.onActionItemClicked(mode, item);
            }

            @Override
            public void onDestroyActionMode(ActionMode mode) {
                callback.onDestroyActionMode(mode);
            }

            @Override
            public void onGetContentRect(ActionMode mode, View view, Rect outRect) {
                reassertCookieTextSelection();
                Log.d(
                        "TextTest",
                        "contentRect request selection=" + nativeCookieEditor.getSelectionStart()
                                + ".." + nativeCookieEditor.getSelectionEnd()
                                + " focused=" + nativeCookieEditor.hasFocus());
                if (callback instanceof ActionMode.Callback2) {
                    ((ActionMode.Callback2) callback)
                            .onGetContentRect(mode, view, outRect);
                } else {
                    outRect.set(0, 0, view.getWidth(), view.getHeight());
                }
                int[] editorScreen = new int[2];
                view.getLocationOnScreen(editorScreen);
                int[] editorWindow = new int[2];
                view.getLocationInWindow(editorWindow);
                int[] rootScreen = new int[2];
                view.getRootView().getLocationOnScreen(rootScreen);
                Rect global = new Rect();
                boolean globalOk = view.getGlobalVisibleRect(global);
                Rect viewport = new Rect();
                getWindow().getDecorView().getWindowVisibleDisplayFrame(viewport);
                int[] decorScreen = new int[2];
                getWindow().getDecorView().getLocationOnScreen(decorScreen);
                Log.d(
                        "GPUICookiePanel",
                        "contentRect=" + outRect
                                + " editorScreen=" + editorScreen[0] + "," + editorScreen[1]
                                + " editorWindow=" + editorWindow[0] + "," + editorWindow[1]
                                + " rootScreen=" + rootScreen[0] + "," + rootScreen[1]
                                + " globalVisible=" + globalOk + ":" + global
                                + " decorScreen=" + decorScreen[0] + "," + decorScreen[1]
                                + " viewport=" + viewport);
            }
        };
    }

    /** ActionMode with a floating cut/copy/paste/select-all toolbar. */
    private class CookieActionMode extends ActionMode implements View.OnClickListener {
        private final ActionMode.Callback callback;
        @SuppressWarnings("unused")
        private final PopupMenu source;
        private final Menu menu;
        private final PopupWindow toolbar;
        private final LinearLayout toolbarRow;
        private boolean started;
        private boolean finished;

        CookieActionMode(ActionMode.Callback callback) {
            this.callback = callback;
            source = new PopupMenu(GpuiActivity.this, nativeCookieEditor);
            menu = source.getMenu();
            float density = nativeCookieDensity;
            toolbarRow = new LinearLayout(GpuiActivity.this);
            toolbarRow.setOrientation(LinearLayout.HORIZONTAL);
            toolbarRow.setGravity(Gravity.CENTER_VERTICAL);
            toolbarRow.setBackground(roundedRect(0xF22B2B2B, 8 * density, 0, 0));
            int pad = Math.round(4 * density);
            toolbarRow.setPadding(pad, pad, pad, pad);
            toolbar = new PopupWindow(toolbarRow,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    false);
            toolbar.setBackgroundDrawable(null);
            toolbar.setOutsideTouchable(true);
            toolbar.setFocusable(false);
            toolbar.setOnDismissListener(this::finish);
        }

        boolean start() {
            if (!callback.onCreateActionMode(this, menu)) {
                return false;
            }
            started = true;
            callback.onPrepareActionMode(this, menu);
            StringBuilder orders = new StringBuilder();
            for (int i = 0; i < menu.size(); i++) {
                MenuItem item = menu.getItem(i);
                orders.append(item.getOrder())
                        .append(':')
                        .append(item.getTitle())
                        .append(':')
                        .append(item.isVisible() ? "v" : "h")
                        .append(' ');
            }
            Log.d("GPUICookiePanel", "action mode menu " + orders);
            rebuildToolbar();
            return true;
        }

        private void rebuildToolbar() {
            toolbarRow.removeAllViews();
            int shown = 0;
            for (int i = 0; i < menu.size() && shown < 4; i++) {
                MenuItem item = menu.getItem(i);
                if (!item.isVisible()) {
                    continue;
                }
                // Keep the core edit items: cut(4), copy(5), paste(6),
                // select all(8). Assist/share/process-text items would not
                // fit a compact floating bar.
                int order = item.getOrder();
                if (order != 4 && order != 5 && order != 6 && order != 8) {
                    continue;
                }
                TextView entry = new TextView(GpuiActivity.this);
                entry.setText(item.getTitle());
                entry.setTextSize(
                        TypedValue.COMPLEX_UNIT_PX, 16 * nativeCookieDensity);
                entry.setTextColor(item.isEnabled() ? 0xFFFFFFFF : 0x66FFFFFF);
                entry.setGravity(Gravity.CENTER);
                entry.setIncludeFontPadding(false);
                int padH = Math.round(16 * nativeCookieDensity);
                entry.setPadding(
                        padH,
                        Math.round(10 * nativeCookieDensity),
                        padH,
                        Math.round(10 * nativeCookieDensity));
                entry.setTag(item);
                entry.setClickable(true);
                entry.setOnClickListener(this);
                toolbarRow.addView(entry, new LinearLayout.LayoutParams(
                        LinearLayout.LayoutParams.WRAP_CONTENT,
                        LinearLayout.LayoutParams.WRAP_CONTENT));
                shown++;
            }
            if (shown == 0) {
                Log.d("GPUICookiePanel", "toolbar empty, dismissing");
                if (toolbar.isShowing()) {
                    toolbar.dismiss();
                }
                return;
            }
            Log.d("GPUICookiePanel", "toolbar entries=" + shown);
            showToolbar();
        }

        private void showToolbar() {
            toolbarRow.measure(
                    View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
                    View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
            int width = toolbarRow.getMeasuredWidth();
            int height = toolbarRow.getMeasuredHeight();
            Rect rect = new Rect();
            if (callback instanceof ActionMode.Callback2) {
                ((ActionMode.Callback2) callback).onGetContentRect(
                        this, nativeCookieEditor, rect);
            }
            if (rect.isEmpty()) {
                rect.set(0, 0, nativeCookieEditor.getWidth(),
                        Math.max(1, nativeCookieEditor.getHeight() / 4));
            }
            int[] location = new int[2];
            nativeCookieEditor.getLocationInWindow(location);
            int margin = Math.round(8 * nativeCookieDensity);
            int x = location[0] + rect.centerX() - width / 2;
            x = Math.max(margin, Math.min(x, nativeCookiePanelWidthPx - width - margin));
            int above = location[1] + rect.top - height - margin;
            int y = above >= margin
                    ? above
                    : location[1] + rect.bottom + margin;
            Log.d(
                    "GPUICookiePanel",
                    "toolbar at=" + x + "," + y
                            + " w=" + width + " h=" + height
                            + " panelWidth=" + nativeCookiePanelWidthPx);
            if (toolbar.isShowing()) {
                toolbar.update(x, y, width, height);
            } else {
                toolbar.showAtLocation(
                        nativeCookieEditor, Gravity.NO_GRAVITY, x, y);
            }
        }

        @Override
        public void onClick(View view) {
            Object tag = view.getTag();
            if (finished || !(tag instanceof MenuItem)) {
                return;
            }
            MenuItem item = (MenuItem) tag;
            if (!item.isEnabled()) {
                return;
            }
            if (callback.onActionItemClicked(this, item)) {
                callback.onPrepareActionMode(this, menu);
                rebuildToolbar();
            }
        }

        @Override
        public void invalidate() {
            if (!started || finished) {
                return;
            }
            callback.onPrepareActionMode(this, menu);
            rebuildToolbar();
        }

        @Override
        public void invalidateContentRect() {
            if (started && !finished) {
                showToolbar();
            }
        }

        @Override
        public void finish() {
            if (finished) {
                return;
            }
            finished = true;
            if (toolbar.isShowing()) {
                toolbar.dismiss();
            }
            if (started) {
                callback.onDestroyActionMode(this);
            }
            if (nativeCookieActionMode == this) {
                nativeCookieActionMode = null;
            }
        }

        @Override
        public void setTitle(CharSequence title) {}

        @Override
        public void setTitle(int resId) {}

        @Override
        public void setSubtitle(CharSequence subtitle) {}

        @Override
        public void setSubtitle(int resId) {}

        @Override
        public void setCustomView(View view) {}

        @Override
        public Menu getMenu() {
            return menu;
        }

        @Override
        public CharSequence getTitle() {
            return null;
        }

        @Override
        public CharSequence getSubtitle() {
            return null;
        }

        @Override
        public View getCustomView() {
            return null;
        }

        @Override
        public MenuInflater getMenuInflater() {
            return new MenuInflater(GpuiActivity.this);
        }
    }

    /**
     * Real EditText that takes over the focused GPUI input: the system owns
     * the keyboard, caret, drag handles, and edit menu; edits and selection
     * changes are forwarded back to Rust.
     */
    private final class MirrorEditText extends EditText {
        private final int touchSlop;
        private float touchDownX;
        private float touchDownY;
        private float lastTouchY;
        private boolean touchMoved;
        private boolean scrollingText;
        private boolean selectionHandleGesture;
        private boolean preserveSelectionGesture;
        private boolean superTouchActive;
        private int scrollingTextTop;
        private int visualScrollTop;
        private int lastNotifiedScrollTop = Integer.MIN_VALUE;
        private boolean scrollNotifyPosted;
        private int pendingSelectionReveal = -1;
        private boolean selectionHandlePositionRefreshPosted;
        private Drawable selectHandleLeftDrawable;
        private Drawable selectHandleRightDrawable;
        private boolean selectHandleLeftHidden;
        private boolean selectHandleRightHidden;
        private String pendingMirrorText;
        private boolean mirrorTextNotifyPosted;
        private String lastMirrorSentText;
        private boolean lastNotifiedNativeFocus;
        private final Runnable mirrorTextNotifyRunnable = new Runnable() {
            @Override
            public void run() {
                mirrorTextNotifyPosted = false;
                flushMirrorTextNotify();
            }
        };
        private final Runnable scrollNotifyRunnable = new Runnable() {
            @Override
            public void run() {
                scrollNotifyPosted = false;
                notifyNativeScroll();
            }
        };
        private final Runnable selectionHandlePositionRefreshRunnable = new Runnable() {
            @Override
            public void run() {
                selectionHandlePositionRefreshPosted = false;
                if (scrollingText
                        || getSelectionStart() == getSelectionEnd()
                        || getVisibility() != View.VISIBLE) {
                    return;
                }
                invalidate();
                android.view.ViewTreeObserver observer = getViewTreeObserver();
                if (observer.isAlive()) {
                    observer.dispatchOnPreDraw();
                }
            }
        };

        /**
         * Selection handles are separate windows positioned by TextView's
         * standard pre-draw position listeners. Programmatic scrolling marks
         * those listeners dirty, but some platform implementations only
         * process the update on the next traversal. Explicitly dispatching
         * pre-draw after scrolling makes every Android implementation
         * recalculate both handles from the final scroll offset.
         */
        private void scheduleSelectionHandlePositionRefresh() {
            if (selectionHandlePositionRefreshPosted) {
                return;
            }
            selectionHandlePositionRefreshPosted = true;
            postOnAnimation(selectionHandlePositionRefreshRunnable);
        }

        private final Runnable revealSelectionRunnable = new Runnable() {
            @Override
            public void run() {
                int offset = pendingSelectionReveal;
                pendingSelectionReveal = -1;
                if (scrollingText
                        || offset < 0
                        || getVisibility() != View.VISIBLE
                        || !hasFocus()
                        || getLayout() == null) {
                    return;
                }
                Layout layout = getLayout();
                int line = layout.getLineForOffset(offset);
                int lineTop = layout.getLineTop(line);
                int lineBottom = layout.getLineBottom(line);
                int paddingTop = getTotalPaddingTop();
                int paddingBottom = getTotalPaddingBottom();
                int maxScroll = Math.max(
                        0,
                        layout.getHeight() + paddingTop + paddingBottom - getHeight());
                int before = visualScrollTop;
                int target = before;
                int viewportTop = paddingTop;
                int viewportBottom = getHeight() - paddingBottom;
                int lineViewTop = paddingTop + lineTop - before;
                int lineViewBottom = lineViewTop + lineBottom - lineTop;
                if (lineViewTop < viewportTop) {
                    target = lineTop;
                } else if (lineViewBottom > viewportBottom) {
                    target = paddingTop + lineBottom - viewportBottom;
                }
                target = Math.max(0, Math.min(target, maxScroll));
                if (target != getScrollY()) {
                    scrollTo(0, target);
                }
                visualScrollTop = target;
                scheduleNativeScrollNotify();
            }
        };

        MirrorEditText(Context context) {
            super(context);
            setFocusable(true);
            setFocusableInTouchMode(true);
            setLongClickable(true);
            touchSlop = ViewConfiguration.get(context).getScaledTouchSlop();
        }

        @Override
        public ActionMode startActionMode(ActionMode.Callback callback) {
            return startActionMode(callback, ActionMode.TYPE_PRIMARY);
        }

        @Override
        public ActionMode startActionMode(ActionMode.Callback callback, int type) {
            // The editor is the root view of its own application window, so
            // View.startActionMode() cannot walk to an Activity DecorView.
            // Delegate directly to that DecorView while retaining this
            // EditText as the originating view. Passing only the Activity
            // makes Android use the whole Activity as the ActionMode anchor
            // and bypasses the TextView's system selection-menu integration.
            if (getWindow() != null
                    && getWindow().getDecorView() instanceof ViewGroup) {
                return ((ViewGroup) getWindow().getDecorView())
                        .startActionModeForChild(this, callback, type);
            }
            return GpuiActivity.this.startActionMode(callback, type);
        }

        void revealSelection(int offset) {
            if (scrollingText) {
                pendingSelectionReveal = -1;
                removeCallbacks(revealSelectionRunnable);
                return;
            }
            pendingSelectionReveal = Math.max(0, offset);
            removeCallbacks(revealSelectionRunnable);
            postOnAnimation(revealSelectionRunnable);
        }

        private void cancelSuperTouch(MotionEvent event) {
            MotionEvent cancel = MotionEvent.obtain(event);
            cancel.setAction(MotionEvent.ACTION_CANCEL);
            super.onTouchEvent(cancel);
            cancel.recycle();
        }

        void resetNotifiedScroll() {
            lastNotifiedScrollTop = Integer.MIN_VALUE;
        }

        void scheduleMirrorTextNotify() {
            pendingMirrorText = getText().toString();
            nativeEditorSnapshot = pendingMirrorText;
            if (mirrorTextNotifyPosted) {
                return;
            }
            mirrorTextNotifyPosted = true;
            // Coalesce edits made in the same display frame without adding a
            // fixed 32 ms delay to every key or paste.
            postOnAnimation(mirrorTextNotifyRunnable);
        }

        void cancelPendingMirrorTextNotify() {
            removeCallbacks(mirrorTextNotifyRunnable);
            mirrorTextNotifyPosted = false;
            pendingMirrorText = null;
        }

        void flushMirrorTextNotify() {
            removeCallbacks(mirrorTextNotifyRunnable);
            mirrorTextNotifyPosted = false;
            String value = pendingMirrorText;
            pendingMirrorText = null;
            if (value == null || getVisibility() != View.VISIBLE) {
                return;
            }
            nativeEditorSnapshot = value;
            lastMirrorSentText = value;
            nativeMirrorTextChanged(
                    value,
                    Math.max(0, getSelectionStart()),
                    Math.max(0, getSelectionEnd()),
                    nativeEditorSession);
        }

        private void hideNativeSelectionHandlesForScroll() {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                return;
            }
            setSelectHandleHidden(true, true);
            setSelectHandleHidden(false, true);
        }

        private void setSelectHandleHidden(boolean startHandle, boolean hidden) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                return;
            }
            boolean currentlyHidden =
                    startHandle ? selectHandleLeftHidden : selectHandleRightHidden;
            if (currentlyHidden == hidden) {
                return;
            }
            if (hidden) {
                Drawable current = startHandle
                        ? getTextSelectHandleLeft()
                        : getTextSelectHandleRight();
                if (current == null) {
                    return;
                }
                if (startHandle) {
                    selectHandleLeftDrawable = current;
                    selectHandleLeftHidden = true;
                    setTextSelectHandleLeft(new TransparentHandleDrawable(current));
                } else {
                    selectHandleRightDrawable = current;
                    selectHandleRightHidden = true;
                    setTextSelectHandleRight(new TransparentHandleDrawable(current));
                }
                return;
            }
            Drawable original = startHandle
                    ? selectHandleLeftDrawable
                    : selectHandleRightDrawable;
            if (startHandle) {
                selectHandleLeftDrawable = null;
                selectHandleLeftHidden = false;
                if (original != null) {
                    setTextSelectHandleLeft(original);
                }
            } else {
                selectHandleRightDrawable = null;
                selectHandleRightHidden = false;
                if (original != null) {
                    setTextSelectHandleRight(original);
                }
            }
        }

        private boolean isSelectionEndpointVisible(int offset) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                return true;
            }
            Layout layout = getLayout();
            if (layout == null || layout.getLineCount() == 0) {
                return false;
            }
            int line = layout.getLineForOffset(offset);
            int viewportOffset = getExtendedPaddingTop() - getScrollY();
            int lineTop = viewportOffset + layout.getLineTop(line);
            int lineBottom = viewportOffset + layout.getLineBottom(line);
            // The handle itself is a separate system window and may extend
            // below the EditText. Only hide it when the selected line has
            // actually scrolled outside the editor viewport.
            return lineBottom > 0 && lineTop < getHeight();
        }

        private void updateSelectionHandleVisibility() {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                return;
            }
            int start = getSelectionStart();
            int end = getSelectionEnd();
            if (scrollingText
                    || getVisibility() != View.VISIBLE
                    || !hasFocus()
                    || start < 0
                    || end < 0
                    || start == end) {
                return;
            }
            setSelectHandleHidden(true, !isSelectionEndpointVisible(start));
            setSelectHandleHidden(false, !isSelectionEndpointVisible(end));
        }

        void restoreNativeSelectionHandles() {
            setSelectHandleHidden(true, false);
            setSelectHandleHidden(false, false);
        }

        private void scheduleNativeScrollNotify() {
            if (scrollNotifyPosted) {
                return;
            }
            scrollNotifyPosted = true;
            postOnAnimation(scrollNotifyRunnable);
        }

        private void flushNativeScrollNotify() {
            removeCallbacks(scrollNotifyRunnable);
            scrollNotifyPosted = false;
            notifyNativeScroll();
        }

        private void notifyNativeScroll() {
            if (visualScrollTop == lastNotifiedScrollTop) {
                return;
            }
            lastNotifiedScrollTop = visualScrollTop;
            nativeMirrorScrollChanged(
                    visualScrollTop / getResources().getDisplayMetrics().density);
        }

        private String lastLoggedLineMap;
        private boolean lineRangesNotifyPosted;
        private final Runnable lineRangesNotify = () -> {
            lineRangesNotifyPosted = false;
            Layout layout = getLayout();
            if (layout == null || getVisibility() != View.VISIBLE) {
                return;
            }
            StringBuilder lineStarts = new StringBuilder();
            for (int line = 0; line < layout.getLineCount(); line++) {
                if (line > 0) {
                    lineStarts.append(',');
                }
                lineStarts.append(layout.getLineStart(line))
                        .append(':')
                        .append(layout.getLineEnd(line));
            }
            String text = getText().toString();
            String snapshot = text + "|" + lineStarts;
            if (snapshot.equals(lastLoggedLineMap)) {
                return;
            }
            lastLoggedLineMap = snapshot;
            nativeMirrorLineRangesChanged(text, lineStarts.toString());
        };

        private void scheduleLineRangesNotify() {
            if (lineRangesNotifyPosted) {
                return;
            }
            lineRangesNotifyPosted = true;
            // Wait until the next display frame so Layout reflects the edit,
            // but do not add extra latency. Native ranges drive caret and
            // selection mapping; the Rust layout no longer waits on them when
            // deciding the editor's height.
            postOnAnimation(lineRangesNotify);
        }

        void notifyLineRanges() {
            scheduleLineRangesNotify();
        }

        void flushLineRangesNotify() {
            removeCallbacks(lineRangesNotify);
            lineRangesNotifyPosted = false;
            lineRangesNotify.run();
        }

        private boolean isNearSelectionHandle(float x, float y) {
            int start = getSelectionStart();
            int end = getSelectionEnd();
            Layout layout = getLayout();
            if (start < 0
                    || end < 0
                    || start == end
                    || layout == null
                    || layout.getLineCount() == 0) {
                return false;
            }

            float density = getResources().getDisplayMetrics().density;
            float horizontalSlop = 32.0f * density;
            float verticalSlop = 40.0f * density;
            float paddingLeft = getTotalPaddingLeft();
            float paddingTop = getTotalPaddingTop();
            int scrollY = getScrollY();
            for (int offset : new int[] {start, end}) {
                int line = layout.getLineForOffset(offset);
                float handleX = paddingLeft + layout.getPrimaryHorizontal(offset);
                float lineTop = paddingTop + layout.getLineTop(line) - scrollY;
                float lineBottom = paddingTop + layout.getLineBottom(line) - scrollY;
                if (Math.abs(x - handleX) <= horizontalSlop
                        && y >= lineTop - verticalSlop
                        && y <= lineBottom + verticalSlop) {
                    return true;
                }
            }
            return false;
        }

        private boolean isTouchInsideSelection(float x, float y) {
            int start = getSelectionStart();
            int end = getSelectionEnd();
            if (start < 0 || end < 0 || start == end) {
                return false;
            }
            int offset = getOffsetForPosition(x, y);
            return offset >= Math.min(start, end)
                    && offset <= Math.max(start, end);
        }

        @Override
        protected void onLayout(
                boolean changed, int left, int top, int right, int bottom) {
            super.onLayout(changed, left, top, right, bottom);
            scheduleLineRangesNotify();
            updateSelectionHandleVisibility();
        }

        @Override
        protected void onScrollChanged(int left, int top, int oldLeft, int oldTop) {
            super.onScrollChanged(left, top, oldLeft, oldTop);
            visualScrollTop = top;
            scheduleNativeScrollNotify();
            updateSelectionHandleVisibility();
        }

        @Override
        protected void onFocusChanged(
                boolean focused, int direction, Rect previouslyFocusedRect) {
            super.onFocusChanged(focused, direction, previouslyFocusedRect);
            android.util.Log.d(
                    "GPUI_NATIVE_INPUT",
                    "editorFocusChanged=" + focused
                            + " visible=" + getVisibility()
                            + " attached=" + isAttachedToWindow());
            applyEditorBackground();
            if (getVisibility() == View.VISIBLE
                    && isAttachedToWindow()
                    && focused != lastNotifiedNativeFocus) {
                lastNotifiedNativeFocus = focused;
                nativeOnNativeEditorFocusChanged(focused);
            }
            if (!focused) {
                // GPUI synchronously snapshots the live native text before
                // it blurs this editor. A frame-batched mirror callback that
                // survives the focus change can arrive after a sibling Clear
                // action and restore the text that was just removed.
                cancelPendingMirrorTextNotify();
                flushLineRangesNotify();
                restoreNativeSelectionHandles();
            } else {
                updateSelectionHandleVisibility();
            }
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN
                    && !nativeEditorFocusableMode) {
                // A first tap focuses the native window in place; do not
                // detach/recreate EditText during the gesture.
                beginNativeEditorFocusing(event.getX(), event.getY());
            }
            boolean handled = super.onTouchEvent(event);
            if (event.getActionMasked() == MotionEvent.ACTION_UP
                    && event.getEventTime() - event.getDownTime()
                            < ViewConfiguration.getLongPressTimeout()
                    && getSelectionStart() == getSelectionEnd()) {
                // Keep tap-to-reopen behavior, but let long press and active
                // selections finish through TextView's native gesture path.
                requestImeForView(this, false);
            }
            return handled;
        }

        @Override
        protected void onSelectionChanged(int selStart, int selEnd) {
            super.onSelectionChanged(selStart, selEnd);
            if (selStart == selEnd && !scrollingText) {
                restoreNativeSelectionHandles();
            } else {
                updateSelectionHandleVisibility();
            }
            if (mirrorSyncing
                    || getVisibility() != View.VISIBLE
                    || !isAttachedToWindow()) {
                return;
            }
            if (selStart == lastMirrorSelectionStart && selEnd == lastMirrorSelectionEnd) {
                return;
            }
            int activeOffset = selEnd;
            if (selStart != lastMirrorSelectionStart && selEnd == lastMirrorSelectionEnd) {
                activeOffset = selStart;
            }
            lastMirrorSelectionStart = selStart;
            lastMirrorSelectionEnd = selEnd;
            revealSelection(activeOffset);
            nativeMirrorSelectionChanged(selStart, selEnd);
        }
    }

    private static final class TransparentHandleDrawable extends Drawable {
        private final int width;
        private final int height;

        TransparentHandleDrawable(Drawable source) {
            width = Math.max(1, source.getIntrinsicWidth());
            height = Math.max(1, source.getIntrinsicHeight());
        }

        @Override
        public void draw(Canvas canvas) {}

        @Override
        public void setAlpha(int alpha) {}

        @Override
        public void setColorFilter(android.graphics.ColorFilter colorFilter) {}

        @Override
        public int getOpacity() {
            return android.graphics.PixelFormat.TRANSPARENT;
        }

        @Override
        public int getIntrinsicWidth() {
            return width;
        }

        @Override
        public int getIntrinsicHeight() {
            return height;
        }
    }

    private static final class GpuiInputView extends View {
        private final Editable editable = new SpannableStringBuilder();
        private final GpuiAccessibilityProvider accessibilityProvider;
        private final EditText mirrorEditor;
        private int inputType = EditorInfo.TYPE_CLASS_TEXT;

        GpuiInputView(Context context, EditText mirrorEditor) {
            super(context);
            this.mirrorEditor = mirrorEditor;
            accessibilityProvider = new GpuiAccessibilityProvider(this);
            setFocusable(false);
            setFocusableInTouchMode(false);
        }

        void setInputType(int value) {
            inputType = value;
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public AccessibilityNodeProvider getAccessibilityNodeProvider() {
            return accessibilityProvider;
        }

        void resetEditable() {
            editable.clear();
            Selection.setSelection(editable, 0);
        }

        private void trimEditable() {
            int excess = InputShadowBufferPolicy.trimPrefixLength(editable.length());
            if (excess > 0) {
                editable.delete(0, excess);
            }
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo attributes) {
            if (mirrorEditor != null && mirrorEditor.getVisibility() == View.VISIBLE) {
                InputConnection nativeConnection =
                        mirrorEditor.onCreateInputConnection(attributes);
                if (nativeConnection != null) {
                    return nativeConnection;
                }
            }
            attributes.inputType = inputType;
            attributes.imeOptions = EditorInfo.IME_ACTION_NONE
                    | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
            attributes.initialSelStart = editable.length();
            attributes.initialSelEnd = editable.length();
            return new BaseInputConnection(this, true) {
                @Override
                public Editable getEditable() {
                    if (mirrorEditor != null
                            && mirrorEditor.getVisibility() == View.VISIBLE) {
                        return mirrorEditor.getText();
                    }
                    return editable;
                }

                @Override
                public boolean commitText(CharSequence text, int newCursorPosition) {
                    nativeCommitText(text == null ? "" : text.toString());
                    boolean committed = super.commitText(text, newCursorPosition);
                    trimEditable();
                    return committed;
                }

                @Override
                public boolean setComposingText(CharSequence text, int newCursorPosition) {
                    nativeSetComposingText(text == null ? "" : text.toString());
                    boolean composing = super.setComposingText(text, newCursorPosition);
                    trimEditable();
                    return composing;
                }

                @Override
                public boolean finishComposingText() {
                    nativeFinishComposingText();
                    return super.finishComposingText();
                }

                @Override
                public boolean deleteSurroundingText(int beforeLength, int afterLength) {
                    nativeDeleteSurroundingText(beforeLength, afterLength);
                    boolean deleted = super.deleteSurroundingText(beforeLength, afterLength);
                    trimEditable();
                    return deleted;
                }
            };
        }
    }

    private static final class GpuiAccessibilityProvider extends AccessibilityNodeProvider {
        private static final int HOST_ID = View.NO_ID;
        private static final int ACTION_CLICK = 1;
        private static final int ACTION_FOCUS = 2;
        private static final int ACTION_INCREMENT = 3;
        private static final int ACTION_DECREMENT = 4;

        private final View host;
        private final Map<Long, Integer> virtualIds = new HashMap<>();
        private final Map<Integer, Long> nodeIds = new HashMap<>();
        private final Map<Long, JSONObject> nodesById = new HashMap<>();
        private final Map<Long, Long> parentIds = new HashMap<>();
        private JSONObject cachedSnapshot;
        private int nextVirtualId = 1;
        private int accessibilityFocusedId = HOST_ID;

        GpuiAccessibilityProvider(View host) {
            this.host = host;
        }

        private JSONObject snapshot() throws JSONException {
            if (cachedSnapshot != null) {
                return cachedSnapshot;
            }
            String json = nativeAccessibilitySnapshot();
            JSONObject snapshot = new JSONObject(json == null || json.isEmpty() ? "{}" : json);
            rebuildNodeIndex(snapshot);
            cachedSnapshot = snapshot;
            return snapshot;
        }

        void invalidateSnapshot() {
            cachedSnapshot = null;
            nodesById.clear();
            parentIds.clear();
        }

        private void rebuildNodeIndex(JSONObject snapshot) throws JSONException {
            nodesById.clear();
            parentIds.clear();
            JSONArray nodes = snapshot.optJSONArray("nodes");
            if (nodes != null) {
                for (int index = 0; index < nodes.length(); index++) {
                    JSONObject node = nodes.getJSONObject(index);
                    if (node.has("id")) {
                        nodesById.put(
                                GpuiAccessibilityNodeId.fromJsonValue(node.get("id")), node);
                    }
                }
                for (Map.Entry<Long, JSONObject> entry : nodesById.entrySet()) {
                    JSONArray children = entry.getValue().optJSONArray("children");
                    if (children == null) {
                        continue;
                    }
                    for (int index = 0; index < children.length(); index++) {
                        parentIds.put(
                                GpuiAccessibilityNodeId.fromJsonValue(children.get(index)),
                                entry.getKey());
                    }
                }
            }

            Iterator<Map.Entry<Long, Integer>> ids = virtualIds.entrySet().iterator();
            while (ids.hasNext()) {
                Map.Entry<Long, Integer> entry = ids.next();
                if (!nodesById.containsKey(entry.getKey())) {
                    nodeIds.remove(entry.getValue());
                    ids.remove();
                }
            }
        }

        private int virtualId(long nodeId) {
            Integer existing = virtualIds.get(nodeId);
            if (existing != null) {
                return existing;
            }
            if (nextVirtualId <= 0) {
                throw new IllegalStateException("Android accessibility virtual IDs are exhausted");
            }
            // AccessKit IDs are unsigned 64-bit values; Android's virtual
            // view IDs are positive 32-bit integers, so keep an explicit map.
            int id = nextVirtualId++;
            virtualIds.put(nodeId, id);
            nodeIds.put(id, nodeId);
            return id;
        }

        private JSONObject findNode(long nodeId) {
            return nodesById.get(nodeId);
        }

        private Long findParent(long nodeId) {
            return parentIds.get(nodeId);
        }

        @Override
        public AccessibilityNodeInfo createAccessibilityNodeInfo(int virtualViewId) {
            try {
                JSONObject snapshot = snapshot();
                if (virtualViewId == HOST_ID) {
                    AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host);
                    host.onInitializeAccessibilityNodeInfo(info);
                    info.setPackageName(host.getContext().getPackageName());
                    info.setClassName(GpuiActivity.class.getName());
                    info.setSource(host);
                    info.setVisibleToUser(true);
                    if (!snapshot.isNull("root")) {
                        JSONObject root = findNode(
                                GpuiAccessibilityNodeId.fromJsonValue(snapshot.get("root")));
                        JSONArray children = root == null ? null : root.optJSONArray("children");
                        if (children != null) {
                            for (int index = 0; index < children.length(); index++) {
                                info.addChild(host, virtualId(
                                        GpuiAccessibilityNodeId.fromJsonValue(children.get(index))));
                            }
                        }
                    }
                    return info;
                }

                Long nodeId = nodeIds.get(virtualViewId);
                if (nodeId == null) {
                    return null;
                }
                JSONObject node = findNode(nodeId);
                if (node == null) {
                    return null;
                }

                AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain();
                info.setPackageName(host.getContext().getPackageName());
                info.setSource(host, virtualViewId);
                Long parent = findParent(nodeId);
                Long rootId = snapshot.isNull("root")
                        ? null
                        : GpuiAccessibilityNodeId.fromJsonValue(snapshot.get("root"));
                if (parent == null || (rootId != null && parent.equals(rootId))) {
                    info.setParent(host);
                } else {
                    info.setParent(host, virtualId(parent));
                }

                String role = node.optString("role", "Unknown");
                info.setClassName(classNameForRole(role));
                String label = node.isNull("label") ? "" : node.optString("label", "");
                String value = node.isNull("value") ? "" : node.optString("value", "");
                String description = node.isNull("description")
                        ? ""
                        : node.optString("description", "");
                info.setContentDescription(label.isEmpty() ? description : label);
                if (!value.isEmpty()) {
                    info.setText(value);
                }
                info.setEnabled(!node.optBoolean("disabled", false));
                info.setVisibleToUser(true);

                JSONArray bounds = node.optJSONArray("bounds");
                if (bounds != null && bounds.length() == 4) {
                    // The Android renderer sends physical pixels, matching
                    // View bounds and getLocationOnScreen coordinates.
                    Rect parentBounds = new Rect(
                            (int) Math.floor(bounds.getDouble(0)),
                            (int) Math.floor(bounds.getDouble(1)),
                            (int) Math.ceil(bounds.getDouble(2)),
                            (int) Math.ceil(bounds.getDouble(3)));
                    info.setBoundsInParent(parentBounds);
                    int[] location = new int[2];
                    host.getLocationOnScreen(location);
                    parentBounds.offset(location[0], location[1]);
                    info.setBoundsInScreen(parentBounds);
                }

                JSONArray children = node.optJSONArray("children");
                if (children != null) {
                    for (int index = 0; index < children.length(); index++) {
                        info.addChild(host, virtualId(
                                GpuiAccessibilityNodeId.fromJsonValue(children.get(index))));
                    }
                }

                boolean clickable = node.optBoolean("click", false);
                boolean focusable = node.optBoolean("focus", false) || clickable;
                info.setClickable(clickable);
                info.setFocusable(focusable);
                if (clickable) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
                }
                if (focusable) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_FOCUS);
                }
                info.addAction(
                        virtualViewId == accessibilityFocusedId
                                ? AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS
                                : AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
                if (node.optBoolean("increment", false)) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                }
                if (node.optBoolean("decrement", false)) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                }
                info.setAccessibilityFocused(virtualViewId == accessibilityFocusedId);
                return info;
            } catch (JSONException error) {
                return null;
            }
        }

        @Override
        public boolean performAction(int virtualViewId, int action, Bundle arguments) {
            Long nodeId = nodeIds.get(virtualViewId);
            if (nodeId == null) {
                return false;
            }
            if (action == AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS) {
                accessibilityFocusedId = virtualViewId;
                sendVirtualEvent(
                        virtualViewId,
                        AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED);
                return nativeAccessibilityAction(nodeId, ACTION_FOCUS);
            }
            if (action == AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS) {
                accessibilityFocusedId = HOST_ID;
                sendVirtualEvent(
                        virtualViewId,
                        AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_CLICK) {
                return nativeAccessibilityAction(nodeId, ACTION_CLICK);
            }
            if (action == AccessibilityNodeInfo.ACTION_FOCUS) {
                return nativeAccessibilityAction(nodeId, ACTION_FOCUS);
            }
            if (action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD) {
                return nativeAccessibilityAction(nodeId, ACTION_INCREMENT);
            }
            if (action == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD) {
                return nativeAccessibilityAction(nodeId, ACTION_DECREMENT);
            }
            return false;
        }

        private void sendVirtualEvent(int virtualViewId, int eventType) {
            AccessibilityEvent event = AccessibilityEvent.obtain(eventType);
            event.setPackageName(host.getContext().getPackageName());
            event.setClassName(GpuiActivity.class.getName());
            event.setSource(host, virtualViewId);
            if (host.getParent() != null) {
                host.getParent().requestSendAccessibilityEvent(host, event);
            }
        }

        private static String classNameForRole(String role) {
            if (role.contains("Button")) {
                return android.widget.Button.class.getName();
            }
            if (role.contains("CheckBox")) {
                return android.widget.CheckBox.class.getName();
            }
            if (role.contains("Switch")) {
                return android.widget.Switch.class.getName();
            }
            if (role.contains("TextInput") || role.contains("SearchInput")) {
                return android.widget.EditText.class.getName();
            }
            if (role.contains("Slider")) {
                return android.widget.SeekBar.class.getName();
            }
            if (role.contains("Image")) {
                return android.widget.ImageView.class.getName();
            }
            return android.widget.TextView.class.getName();
        }
    }
}
