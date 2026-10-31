package app.inkpdf.driver;

import android.app.Instrumentation;
import android.app.UiAutomation;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.InputDevice;
import android.view.MotionEvent;

import java.util.ArrayList;
import java.util.List;

/**
 * The real-input checks' hands on Android: a pen and fingers (one or several, so a pinch) sent through the system's input dispatcher as
 * MotionEvents, the way UiAutomator does, with the right tool type, pressure and contact size. `adb shell input` can send one pointer only.
 *
 * It is an instrumentation of this package (not of InkPDF, whose process an instrumentation would restart). Run it with
 * `adb shell am instrument -w -e ops 'D 0 pen 100 200 0.4;M 0 110 200 0.5;F;U 0' app.inkpdf.driver/.Drive`; the ops, separated by semicolons, are:
 *   D id tool x y pressure   a pointer goes down (tool: finger or pen); the first of a gesture is ACTION_DOWN, the others ACTION_POINTER_DOWN
 *   M id x y pressure        where a pointer is now (nothing is sent until F)
 *   F                        send ACTION_MOVE with every pointer as it is
 *   U id                     a pointer goes up
 *   W ms                     wait
 */
public class Drive extends Instrumentation {
    private static final class Pointer {
        int id;
        int tool;
        float x, y, pressure;
    }

    private final List<Pointer> down = new ArrayList<>();
    private long downTime;
    private UiAutomation ui;
    private int source;

    private Bundle arguments;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments;
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        Bundle result = new Bundle();
        int code = 0;
        try {
            ui = getUiAutomation();
            for (String op : arguments.getString("ops").split(";")) {
                if (!op.trim().isEmpty()) {
                    run(op.trim().split("\\s+"));
                }
            }
            result.putString("result", "ok");
        } catch (Exception e) {
            code = 1;
            result.putString("result", "failed: " + e);
        }
        finish(code, result);
    }

    private Pointer find(int id) {
        for (Pointer p : down) {
            if (p.id == id) {
                return p;
            }
        }
        return null;
    }

    private void run(String[] op) throws Exception {
        switch (op[0]) {
            case "D": {
                Pointer p = new Pointer();
                p.id = Integer.parseInt(op[1]);
                p.tool = "pen".equals(op[2]) ? MotionEvent.TOOL_TYPE_STYLUS : MotionEvent.TOOL_TYPE_FINGER;
                p.x = Float.parseFloat(op[3]);
                p.y = Float.parseFloat(op[4]);
                p.pressure = Float.parseFloat(op[5]);
                boolean first = down.isEmpty();
                if (first) {
                    downTime = SystemClock.uptimeMillis();
                    source = p.tool == MotionEvent.TOOL_TYPE_STYLUS ? (InputDevice.SOURCE_STYLUS | InputDevice.SOURCE_TOUCHSCREEN) : InputDevice.SOURCE_TOUCHSCREEN;
                }
                down.add(p);
                send(first ? MotionEvent.ACTION_DOWN : (MotionEvent.ACTION_POINTER_DOWN | (down.size() - 1) << MotionEvent.ACTION_POINTER_INDEX_SHIFT));
                break;
            }
            case "M": {
                Pointer p = find(Integer.parseInt(op[1]));
                if (p != null) {
                    p.x = Float.parseFloat(op[2]);
                    p.y = Float.parseFloat(op[3]);
                    p.pressure = Float.parseFloat(op[4]);
                }
                break;
            }
            case "F":
                send(MotionEvent.ACTION_MOVE);
                break;
            case "U": {
                Pointer p = find(Integer.parseInt(op[1]));
                if (p != null) {
                    int index = down.indexOf(p);
                    send(down.size() == 1 ? MotionEvent.ACTION_UP : (MotionEvent.ACTION_POINTER_UP | index << MotionEvent.ACTION_POINTER_INDEX_SHIFT));
                    down.remove(p);
                }
                break;
            }
            case "W":
                SystemClock.sleep(Long.parseLong(op[1]));
                break;
            default:
                throw new IllegalArgumentException("unknown op " + op[0]);
        }
    }

    private void send(int action) {
        int n = down.size();
        MotionEvent.PointerProperties[] props = new MotionEvent.PointerProperties[n];
        MotionEvent.PointerCoords[] coords = new MotionEvent.PointerCoords[n];
        for (int i = 0; i < n; i++) {
            Pointer p = down.get(i);
            props[i] = new MotionEvent.PointerProperties();
            props[i].id = p.id;
            props[i].toolType = p.tool;
            coords[i] = new MotionEvent.PointerCoords();
            coords[i].x = p.x;
            coords[i].y = p.y;
            coords[i].pressure = p.pressure;
            coords[i].size = 0.05f;
            // (A finger's contact is a few millimetres, far from a palm's.)
            coords[i].touchMajor = p.tool == MotionEvent.TOOL_TYPE_FINGER ? 40f : 0f;
            coords[i].touchMinor = coords[i].touchMajor;
        }
        long now = SystemClock.uptimeMillis();
        MotionEvent event = MotionEvent.obtain(downTime, now, action, n, props, coords, 0, 0, 1f, 1f, 0, 0, source, 0);
        ui.injectInputEvent(event, true);
        event.recycle();
    }
}
