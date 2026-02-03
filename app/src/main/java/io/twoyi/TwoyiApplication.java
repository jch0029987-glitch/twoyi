package io.twoyi;

import android.app.Application;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.content.Context;
import android.content.Intent;
import android.content.res.Resources;
import android.os.Build;
import android.util.Log;

import com.microsoft.appcenter.AppCenter;
import com.microsoft.appcenter.analytics.Analytics;
import com.microsoft.appcenter.crashes.Crashes;

import java.lang.reflect.Field;

import io.twoyi.utils.RomManager;
import me.weishu.reflection.Reflection;

/**
 * Main Application class for Twoyi.
 * Updated for Android 14 (API 34) compatibility.
 */
public class TwoyiApplication extends Application {

    private static final String TAG = "TwoyiApp";
    private static final String ENGINE_CHANNEL_ID = "twoyi_engine";
    private static int statusBarHeight = -1;

    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(base);

        // 1. Bypass Hidden API restrictions for Android 9 - 14
        // Essential for Twoyi to access internal hardware rendering classes
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            Reflection.unseal(base);
        }

        // 2. Prepare ROM files in internal storage
        RomManager.ensureBootFiles(base);

        // 3. Initialize the native socket bridge
        try {
            TwoyiSocketServer.getInstance(base).start();
        } catch (Exception e) {
            Log.e(TAG, "Failed to initialize SocketServer", e);
        }
    }

    @Override
    public void onCreate() {
        super.onCreate();

        // 4. Create Notification Channel (Required for Android 8.0+)
        createNotificationChannel();

        // 5. Start Foreground Service Engine (Required for Android 14)
        // This prevents the OS from killing the Rust background processes.
        try {
            Intent engineIntent = new Intent(this, TwoyiEngineService.class);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(engineIntent);
            } else {
                startService(engineIntent);
            }
        } catch (Exception e) {
            Log.e(TAG, "Could not start Twoyi Engine Service", e);
        }

        // 6. Initialize Telemetry (Only in Release builds)
        AppCenter.start(this, "6223c2b1-30ab-4293-8456-ac575420774e",
                Analytics.class, Crashes.class);
        if (BuildConfig.DEBUG) {
            AppCenter.setEnabled(false);
        }
    }

    /**
     * Creates the notification channel required for the Foreground Service.
     */
    private void createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(
                    ENGINE_CHANNEL_ID,
                    "Twoyi Container Engine",
                    NotificationManager.IMPORTANCE_LOW
            );
            channel.setDescription("Ensures the container stays running in the background.");
            NotificationManager manager = getSystemService(NotificationManager.class);
            if (manager != null) {
                manager.createNotificationChannel(channel);
            }
        }
    }

    /**
     * Utility to get status bar height, compatible with notched displays and modern APIs.
     */
    public static int getStatusBarHeight(Context context) {
        if (statusBarHeight != -1) return statusBarHeight;

        // Try standard resource fetching
        int resId = context.getResources().getIdentifier("status_bar_height", "dimen", "android");
        if (resId > 0) {
            statusBarHeight = context.getResources().getDimensionPixelSize(resId);
        }

        // Fallback to Reflection if system resources are hidden
        if (statusBarHeight <= 0) {
            try {
                Class<?> clazz = Class.forName("com.android.internal.R$dimen");
                Object obj = clazz.newInstance();
                Field field = clazz.getField("status_bar_height");
                int resourceId = Integer.parseInt(field.get(obj).toString());
                statusBarHeight = context.getResources().getDimensionPixelSize(resourceId);
            } catch (Exception e) {
                statusBarHeight = dip2px(context, 24); // Reasonable default
            }
        }
        return statusBarHeight;
    }

    private static int dip2px(Context context, float dpValue) {
        float scale = context.getResources().getDisplayMetrics().density;
        return (int) (dpValue * scale + 0.5f);
    }

    public static float px2dp(float pxValue) {
        return (pxValue / Resources.getSystem().getDisplayMetrics().density);
    }
}
