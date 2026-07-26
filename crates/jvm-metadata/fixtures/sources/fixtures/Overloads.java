package fixtures;

public abstract class Overloads {
    public String value(int input) {
        return Integer.toString(input);
    }

    public String value(String input) {
        return input;
    }

    protected static native long nativeValue(long input);
}
