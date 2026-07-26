package fixtures;

import java.io.IOException;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.util.List;

@Retention(RetentionPolicy.RUNTIME)
@interface Marker {
    String value();
}

@Marker("class")
public class RichFixture<T extends Number> implements Runnable {
    public static final int CONSTANT = 7;
    public T value;

    public RichFixture(T value) {
        this.value = value;
    }

    @Marker("method")
    public List<String> combine(String prefix, int... values) throws IOException {
        return List.of(prefix + values.length);
    }

    @Override
    public void run() {
    }

    public class Inner {
        public T get() {
            return value;
        }
    }
}

enum Shade {
    RED,
    BLUE
}

record Point(int x, int y) {
}
