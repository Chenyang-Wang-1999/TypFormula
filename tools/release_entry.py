"""PyInstaller entry: absolute package imports also work outside the repository."""
import sys

if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--self-test":
        from desktop.release_smoke import main
        raise SystemExit(main(sys.argv[2]))
    from desktop.__main__ import main
    raise SystemExit(main())
