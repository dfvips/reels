package com.dreamfly.reels

import android.app.Activity
import android.content.ContentValues
import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.ImageDecoder
import android.graphics.Matrix
import android.media.ExifInterface
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.MediaStore
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.inputmethod.InputMethodManager
import android.webkit.WebView
import android.widget.FrameLayout
import android.widget.ImageView
import androidx.activity.enableEdgeToEdge
import androidx.activity.OnBackPressedCallback
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.view.WindowCompat
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private var lastImeVisibleAt = 0L
  @Volatile
  private var frontendReady = false
  private var startupSplashRequested = false
  private var startupSplash: View? = null
  private var resumeCoverNeeded = false
  private var resumeCover: View? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    startupSplashRequested = intent?.getBooleanExtra(
      EXTRA_SHOW_STARTUP_SPLASH,
      !isHomeRendered(),
    ) ?: !isHomeRendered()
    if (!startupSplashRequested && isHomeRendered()) resumeCoverNeeded = true
    if (!startupSplashRequested) frontendReady = true
    val splashScreen = installSplashScreen()
    splashScreen.setKeepOnScreenCondition { !frontendReady }
    currentActivity = this
    enableEdgeToEdge()
    setSystemBars(this, isSystemDark())
    if (startupSplashRequested) showStartupSplash()
    super.onCreate(savedInstanceState)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (SystemClock.uptimeMillis() - lastImeVisibleAt < 600) {
          lastImeVisibleAt = 0
          return
        }
        val target = webView
        if (target == null) {
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
          return
        }
        target.evaluateJavascript(
          "window.__REELS_BACK__ && window.__REELS_BACK__()",
          null,
        )
      }
    })
    window.decorView.setOnApplyWindowInsetsListener { view, insets ->
      if (
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
        insets.isVisible(WindowInsets.Type.ime())
      ) {
        lastImeVisibleAt = SystemClock.uptimeMillis()
      }
      cachedInsets = formatInsets(view.context, insets)
      view.onApplyWindowInsets(insets)
    }
    window.decorView.requestApplyInsets()
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
    if (startupSplashRequested && startupSplash == null && !isHomeRendered()) {
      showStartupSplash()
    }
  }

  override fun onNewIntent(intent: android.content.Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    startupSplashRequested = false
    homeRendered = true
    startupSplash?.let { overlay ->
      startupSplash = null
      (overlay.parent as? ViewGroup)?.removeView(overlay)
    }
    frontendReady = true
  }

  override fun dispatchKeyEvent(event: KeyEvent): Boolean {
    if (
      event.keyCode == KeyEvent.KEYCODE_BACK &&
      event.action == KeyEvent.ACTION_UP &&
      isImeVisible()
    ) {
      getSystemService(InputMethodManager::class.java)?.hideSoftInputFromWindow(
        webView?.windowToken ?: window.decorView.windowToken,
        0,
      )
      return true
    }
    return super.dispatchKeyEvent(event)
  }

  private fun isImeVisible(): Boolean {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      return window.decorView.rootWindowInsets?.isVisible(WindowInsets.Type.ime()) == true
    }
    val frame = android.graphics.Rect()
    window.decorView.getWindowVisibleDisplayFrame(frame)
    val decorHeight = window.decorView.height
    return decorHeight > 0 && decorHeight - frame.bottom > decorHeight * 0.15f
  }

  override fun onDestroy() {
    resumeCover?.let { cover ->
      (cover.parent as? ViewGroup)?.removeView(cover)
    }
    resumeCover = null
    if (currentActivity === this) currentActivity = null
    super.onDestroy()
  }

  override fun onStop() {
    super.onStop()
    if (isHomeRendered()) resumeCoverNeeded = true
  }

  override fun onStart() {
    super.onStart()
    if (resumeCoverNeeded) showResumeCover()
  }

  private fun showResumeCover() {
    if (resumeCover != null) return
    val decor = window.decorView as? ViewGroup ?: return
    val cover = FrameLayout(this).apply {
      setBackgroundColor(getColor(R.color.launch_background))
      isClickable = true
      isFocusable = true
    }
    decor.addView(
      cover,
      FrameLayout.LayoutParams(
        ViewGroup.LayoutParams.MATCH_PARENT,
        ViewGroup.LayoutParams.MATCH_PARENT,
      ),
    )
    resumeCover = cover
  }

  private fun hideResumeCoverNow() {
    val cover = resumeCover ?: return
    resumeCover = null
    resumeCoverNeeded = false
    (cover.parent as? ViewGroup)?.removeView(cover)
  }

  private fun showStartupSplash() {
    val decor = window.decorView as? ViewGroup ?: return
    val density = resources.displayMetrics.density
    val logoSize = (288 * density).toInt()
    val overlay = FrameLayout(this).apply {
      setBackgroundColor(getColor(R.color.launch_background))
      isClickable = true
      isFocusable = true
    }
    overlay.addView(
      ImageView(this).apply {
        setImageResource(R.drawable.splash_logo)
        scaleType = ImageView.ScaleType.FIT_CENTER
      },
      FrameLayout.LayoutParams(logoSize, logoSize, Gravity.CENTER),
    )
    decor.addView(
      overlay,
      FrameLayout.LayoutParams(
        ViewGroup.LayoutParams.MATCH_PARENT,
        ViewGroup.LayoutParams.MATCH_PARENT,
      ),
    )
    startupSplash = overlay
    Handler(Looper.getMainLooper()).postDelayed(
      { hideStartupSplash(this) },
      8000,
    )
  }

  private fun hideStartupSplashNow() {
    val overlay = startupSplash ?: return
    startupSplash = null
    frontendReady = true
    homeRendered = true
    overlay.animate()
      .alpha(0f)
      .setDuration(180)
      .withEndAction {
        (overlay.parent as? ViewGroup)?.removeView(overlay)
      }
      .start()
  }

  private fun hideStartupSplashAfterPaint() {
    if (startupSplash == null) return
    var hidden = false
    val hide = {
      if (!hidden) {
        hidden = true
        hideStartupSplashNow()
      }
    }
    val target = webView
    if (target == null) {
      Handler(Looper.getMainLooper()).postDelayed(hide, 80)
      return
    }
    target.postVisualStateCallback(
      SystemClock.uptimeMillis(),
      object : WebView.VisualStateCallback() {
        override fun onComplete(requestId: Long) {
          Handler(Looper.getMainLooper()).post(hide)
        }
      },
    )
    Handler(Looper.getMainLooper()).postDelayed(hide, 800)
  }

  override fun onResume() {
    super.onResume()
    if (resumeCover != null) {
      window.decorView.postOnAnimation {
        window.decorView.postOnAnimation {
          hideResumeCoverNow()
        }
      }
    }
    Handler(Looper.getMainLooper()).postDelayed(
      { runCatching { LauncherProxyActivity.applySystemLauncherIcon(this) } },
      500,
    )
    setSystemBars(this, isSystemDark())
    webView?.evaluateJavascript(
      "window.__REELS_SYNC_CHROME__ && window.__REELS_SYNC_CHROME__()",
      null,
    )
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    Handler(Looper.getMainLooper()).postDelayed(
      { runCatching { LauncherProxyActivity.applySystemLauncherIcon(this) } },
      500,
    )
  }

  @Deprecated("Android back is forwarded to the shared Vue navigation state")
  override fun onBackPressed() {
    val target = webView
    if (target == null) {
      super.onBackPressed()
      return
    }
    target.evaluateJavascript(
      "window.__REELS_BACK__ && window.__REELS_BACK__()",
      null,
    )
  }

  fun saveMediaToGallery(path: String): Boolean {
    val file = File(path)
    if (!file.isFile) return false
    val name = file.name
    val dot = name.lastIndexOf('.')
    val extension = if (dot >= 0) name.substring(dot + 1).lowercase() else ""
    val video: Boolean
    val mime: String
    when (extension) {
      "mp4", "m4v", "mov" -> {
        video = true
        mime = "video/$extension"
      }
      "jpeg", "jpg" -> {
        video = false
        mime = "image/jpeg"
      }
      "png" -> {
        video = false
        mime = "image/png"
      }
      "webp" -> {
        video = false
        mime = "image/webp"
      }
      "gif" -> {
        video = false
        mime = "image/gif"
      }
      "heic" -> {
        video = false
        mime = "image/heic"
      }
      else -> {
        video = false
        mime = "application/octet-stream"
      }
    }

    val context = applicationContext
    val values = ContentValues()
    values.put(MediaStore.MediaColumns.DISPLAY_NAME, name)
    values.put(MediaStore.MediaColumns.MIME_TYPE, mime)
    if (Build.VERSION.SDK_INT >= 29) {
      values.put(
        MediaStore.MediaColumns.RELATIVE_PATH,
        if (video) "Movies/Reels" else "Pictures/Reels",
      )
      values.put(MediaStore.MediaColumns.IS_PENDING, 1)
    }

    val uri: Uri = context.contentResolver.insert(
      if (video) {
        MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
      } else {
        MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
      },
      values,
    ) ?: return false

    return try {
      FileInputStream(file).use { input ->
        context.contentResolver.openOutputStream(uri).use { output ->
          if (output == null) return false
          val buffer = ByteArray(64 * 1024)
          while (true) {
            val length = input.read(buffer)
            if (length <= 0) break
            output.write(buffer, 0, length)
          }
        }
      }
      if (Build.VERSION.SDK_INT >= 29) {
        values.clear()
        values.put(MediaStore.MediaColumns.IS_PENDING, 0)
        context.contentResolver.update(uri, values, null, null)
      }
      true
    } catch (error: IOException) {
      context.contentResolver.delete(uri, null, null)
      false
    }
  }

  fun convertImageToJpeg(
    sourcePath: String,
    targetPath: String,
    maxDimension: Int,
    quality: Int,
  ): Boolean {
    val source = File(sourcePath)
    if (!source.isFile) return false
    val bitmap = decodeImage(source, maxDimension) ?: return false
    return try {
      FileOutputStream(targetPath).use { output ->
        bitmap.compress(
          Bitmap.CompressFormat.JPEG,
          quality.coerceIn(1, 100),
          output,
        )
      }
    } catch (_: IOException) {
      false
    } finally {
      bitmap.recycle()
    }
  }

  private fun decodeImage(source: File, maxDimension: Int): Bitmap? {
    if (Build.VERSION.SDK_INT >= 28) {
      try {
        val decoderSource = ImageDecoder.createSource(source)
        return ImageDecoder.decodeBitmap(decoderSource) { decoder, info, _ ->
          decoder.setAllocator(ImageDecoder.ALLOCATOR_SOFTWARE)
          decoder.setTargetSampleSize(
            sampleSize(
              info.size.width,
              info.size.height,
              maxDimension,
            ),
          )
        }
      } catch (_: Throwable) {
        // Fall through to BitmapFactory for vendor decoder failures.
      }
    }
    val bounds = BitmapFactory.Options()
    bounds.inJustDecodeBounds = true
    BitmapFactory.decodeFile(source.absolutePath, bounds)
    val options = BitmapFactory.Options()
    options.inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight, maxDimension)
    val bitmap = BitmapFactory.decodeFile(source.absolutePath, options) ?: return null
    return applyOrientation(bitmap, readOrientation(source.absolutePath))
  }

  private fun sampleSize(width: Int, height: Int, maxDimension: Int): Int {
    var sample = 1
    val longest = maxOf(width, height)
    if (maxDimension > 0) {
      while (longest / (sample * 2) >= maxDimension) sample *= 2
    }
    return sample
  }

  private fun readOrientation(path: String): Int {
    return try {
      ExifInterface(path).getAttributeInt(
        ExifInterface.TAG_ORIENTATION,
        ExifInterface.ORIENTATION_NORMAL,
      )
    } catch (_: IOException) {
      ExifInterface.ORIENTATION_NORMAL
    } catch (_: RuntimeException) {
      ExifInterface.ORIENTATION_NORMAL
    }
  }

  private fun applyOrientation(bitmap: Bitmap, orientation: Int): Bitmap {
    val matrix = Matrix()
    when (orientation) {
      ExifInterface.ORIENTATION_ROTATE_90 -> matrix.postRotate(90f)
      ExifInterface.ORIENTATION_ROTATE_180 -> matrix.postRotate(180f)
      ExifInterface.ORIENTATION_ROTATE_270 -> matrix.postRotate(270f)
      ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> matrix.postScale(-1f, 1f)
      ExifInterface.ORIENTATION_FLIP_VERTICAL -> matrix.postScale(1f, -1f)
      else -> return bitmap
    }
    val rotated = Bitmap.createBitmap(
      bitmap,
      0,
      0,
      bitmap.width,
      bitmap.height,
      matrix,
      true,
    )
    if (rotated !== bitmap) bitmap.recycle()
    return rotated
  }

  companion object {
    private const val EXTRA_SHOW_STARTUP_SPLASH = "show_startup_splash"
    @Volatile
    private var homeRendered = false
    @Volatile
    private var cachedInsets = "0,0,0,0,0"
    @Volatile
    private var currentActivity: MainActivity? = null

    private fun isSystemDark(): Boolean {
      val night = android.content.res.Resources.getSystem().configuration.uiMode and
        Configuration.UI_MODE_NIGHT_MASK
      return night == Configuration.UI_MODE_NIGHT_YES
    }

    @JvmStatic
    @Suppress("DEPRECATION")
    fun setSystemBars(context: Context, dark: Boolean) {
      val activity = currentActivity ?: context as? Activity ?: return
      activity.runOnUiThread {
        val window = activity.window
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
          window.isNavigationBarContrastEnforced = false
        }
        val controller = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          window.insetsController
        } else {
          null
        }
        if (controller != null) {
          val mask =
            WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
              WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
          controller.setSystemBarsAppearance(if (dark) 0 else mask, mask)
        } else {
          val lightBars = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
            View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
          var flags = window.decorView.systemUiVisibility
          flags = if (dark) flags and lightBars.inv() else flags or lightBars
          window.decorView.systemUiVisibility = flags
        }
      }
    }

    @JvmStatic
    fun safeAreaInsets(context: Context): String {
      val activity = currentActivity ?: context as? Activity ?: return "0,0,0,0,0"
      if (Looper.myLooper() == Looper.getMainLooper()) {
        return readInsets(activity)
      }
      var value = "0,0,0,0,0"
      val latch = CountDownLatch(1)
      Handler(Looper.getMainLooper()).post {
        value = readInsets(activity)
        latch.countDown()
      }
      return if (latch.await(120, TimeUnit.MILLISECONDS)) value else "0,0,0,0,0"
    }

    private fun readInsets(activity: Activity): String {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
        val density = activity.resources.displayMetrics.density
        val rect = android.graphics.Rect()
        activity.window.decorView.getWindowVisibleDisplayFrame(rect)
        val top = rect.top.toFloat() / density
        return "0,$top,0,0,0"
      }
      val root = activity.window.decorView.rootWindowInsets
      if (root != null) {
        val live = formatInsets(activity, root)
        if (live != "0,0,0,0,0") return live
      }
      return cachedInsets
    }

    private fun formatInsets(context: Context, root: WindowInsets): String {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return "0,0,0,0,0"
      val bars = root.getInsets(
        WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout(),
      )
      val ime = root.getInsets(WindowInsets.Type.ime())
      val density = context.resources.displayMetrics.density
      val resources = context.resources
      val statusBarId = resources.getIdentifier("status_bar_height", "dimen", "android")
      val statusBarHeight = if (statusBarId > 0) {
        resources.getDimensionPixelSize(statusBarId)
      } else {
        0
      }
      var top = bars.top
      val frame = android.graphics.Rect()
      (context as? Activity)?.window?.decorView?.getWindowVisibleDisplayFrame(frame)
      if (top <= 0) {
        top = maxOf(frame.top, statusBarHeight)
      }
      val decorHeight = (context as? Activity)?.window?.decorView?.height ?: 0
      val keyboardFromFrame = if (frame.bottom > 0 && decorHeight > frame.bottom) {
        decorHeight - frame.bottom
      } else {
        0
      }
      val keyboard = maxOf(ime.bottom, keyboardFromFrame)
      return listOf(
        bars.left / density,
        top / density,
        bars.right / density,
        bars.bottom / density,
        keyboard / density,
      ).joinToString(",")
    }

    @JvmStatic
    fun exitApp(context: Context) {
      val activity = currentActivity ?: context as? Activity ?: return
      activity.runOnUiThread {
        activity.finishAffinity()
      }
    }

    @JvmStatic
    fun hideStartupSplash(context: Context) {
      val activity = currentActivity ?: context as? MainActivity ?: return
      activity.runOnUiThread {
        activity.hideStartupSplashAfterPaint()
      }
    }

    @JvmStatic
    fun peek(): MainActivity? = currentActivity

    @JvmStatic
    fun isHomeRendered(): Boolean = homeRendered

    @JvmStatic
    fun setHomeRendered(rendered: Boolean) {
      homeRendered = rendered
    }

    @JvmStatic
    fun saveMediaToGalleryFromContext(context: Context, path: String): Boolean {
      val activity = currentActivity ?: context as? MainActivity ?: return false
      return activity.saveMediaToGallery(path)
    }

    @JvmStatic
    fun convertImageToJpegFromContext(
      context: Context,
      sourcePath: String,
      targetPath: String,
      maxDimension: Int,
      quality: Int,
    ): Boolean {
      val activity = currentActivity ?: context as? MainActivity ?: return false
      return activity.convertImageToJpeg(
        sourcePath,
        targetPath,
        maxDimension,
        quality,
      )
    }
  }
}
