package com.dreamfly.reels;

import android.app.Activity;
import android.app.ActivityManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.os.Bundle;
import androidx.core.splashscreen.SplashScreen;
import androidx.core.splashscreen.SplashScreenViewProvider;

public final class LauncherProxyActivity extends Activity {
    private static volatile boolean launcherIconSwitchUnavailable = false;

    @Override
    protected void onCreate(Bundle state) {
        boolean warmResume = MainActivity.isHomeRendered();
        if (warmResume) {
            try {
                setTheme(android.R.style.Theme_NoDisplay);
                SplashScreen.installSplashScreen(this)
                        .setOnExitAnimationListener(SplashScreenViewProvider::remove);
            } catch (Throwable error) {
                android.util.Log.w("Reels", "Splash override unavailable", error);
            }
        }
        super.onCreate(state);
        overridePendingTransition(0, 0);
        boolean moved = warmResume && bringMainTaskToFront();
        if (moved) {
            finish();
            return;
        }
        Intent launch = new Intent(this, MainActivity.class);
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_NO_ANIMATION);
        launch.putExtra("from_launcher_proxy", true);
        launch.putExtra("show_startup_splash", !warmResume);
        startActivity(launch);
        overridePendingTransition(0, 0);
        finish();
    }

    private boolean bringMainTaskToFront() {
        try {
            ActivityManager manager = (ActivityManager) getSystemService(ACTIVITY_SERVICE);
            if (manager == null) return false;
            String mainClass = MainActivity.class.getName();
            for (ActivityManager.AppTask task : manager.getAppTasks()) {
                ActivityManager.RecentTaskInfo info = task.getTaskInfo();
                ComponentName base = info == null ? null : info.baseActivity;
                ComponentName top = info == null ? null : info.topActivity;
                boolean isMainTask = (base != null && mainClass.equals(base.getClassName()))
                        || (top != null && mainClass.equals(top.getClassName()));
                if (!isMainTask) continue;
                task.moveToFront();
                return true;
            }
        } catch (Throwable error) {
            android.util.Log.w("Reels", "Unable to bring main task to front", error);
        }
        return false;
    }

    @Override
    public void onBackPressed() {
    }

    public static void applySystemLauncherIcon(Context context) {
        if (launcherIconSwitchUnavailable) return;
        try {
            int night = android.content.res.Resources.getSystem().getConfiguration().uiMode
                    & Configuration.UI_MODE_NIGHT_MASK;
            boolean dark = night == Configuration.UI_MODE_NIGHT_YES;
            String pkg = context.getPackageName();
            ComponentName light = new ComponentName(pkg, pkg + ".LauncherLight");
            ComponentName darkAlias = new ComponentName(pkg, pkg + ".LauncherDark");
            PackageManager manager = context.getPackageManager();
            int lightState = manager.getComponentEnabledSetting(light);
            int darkState = manager.getComponentEnabledSetting(darkAlias);
            boolean lightEnabled = lightState == PackageManager.COMPONENT_ENABLED_STATE_ENABLED
                    || lightState == PackageManager.COMPONENT_ENABLED_STATE_DEFAULT;
            boolean darkEnabled = darkState == PackageManager.COMPONENT_ENABLED_STATE_ENABLED;
            boolean desiredEnabled = dark ? darkEnabled : lightEnabled;
            boolean otherDisabled = dark ? !lightEnabled : !darkEnabled;
            if (desiredEnabled && otherDisabled) {
                return;
            }
            ComponentName desired = dark ? darkAlias : light;
            ComponentName other = dark ? light : darkAlias;
            if (!desiredEnabled) {
                manager.setComponentEnabledSetting(desired,
                        PackageManager.COMPONENT_ENABLED_STATE_ENABLED, PackageManager.DONT_KILL_APP);
            }
            if (!otherDisabled) {
                manager.setComponentEnabledSetting(other,
                        PackageManager.COMPONENT_ENABLED_STATE_DISABLED, PackageManager.DONT_KILL_APP);
            }
        } catch (Throwable error) {
            launcherIconSwitchUnavailable = true;
            android.util.Log.w("Reels", "Launcher icon switching unavailable", error);
        }
    }
}
