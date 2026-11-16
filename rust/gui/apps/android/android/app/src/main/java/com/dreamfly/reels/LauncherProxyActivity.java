package com.dreamfly.reels;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/**
 * Invisible launcher trampoline.
 *
 * <p>The launcher icon is provided by an activity-alias so the icon can change
 * with the system theme. Disabling the alias that launched the task directly
 * would finish that task, so the alias targets this no-display activity
 * instead. It starts the real GPUI activity and immediately finishes, leaving
 * the alias change free to happen without closing the app.</p>
 */
public final class LauncherProxyActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        Intent launch = new Intent(this, dev.gpui.mobile.GpuiActivity.class);
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(launch);
        finish();
    }
}
