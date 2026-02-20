# cyoa

A scripting language, runtime, and HTTP server for choose-your-own-adventure style interactive fiction, written in Rust.

## usage

To start the server, run:

```rust
cargo run -- --source path/to/story.cyoa [--port 8080]
```

Or run the binary directly:

```bash
./cyoa --source path/to/story.cyoa [--port 8080]
```

If no port is specified, the server will choose a random available port.
The port number is written to `port.json`.
A client can then interact with the story by sending HTTP requests to the server.

## api

- `GET /current`: returns the current node (text + available choices + whether the story is over)
    - Response format:
    ```json
    {
        "display_text": "Narration text to be displayed to the user.",
        "choices": [
            {
                "display_text": "Text to be displayed for this choice.",
                "id": "The ID of this choice"
            }
        ],
        "game_over": false
    }
    ```
- `POST /choose/{choice_id}`: advance the story by selecting the choice with the given ID

## story format

Stories are written in `.cyoa` files:

```
SET name "my friend"
SET x 0

= START
    "Hello, {name}! Left or right?"
    "Go left." -> left_path
    [IF x > 0] "Go right." -> right_path

= left_path
    "You went left."
    "Go back." -> START [THEN x = 1]

= right_path
    "You went right."
```

Notes:

- `SET`: define a variable
- `= name`: define a scene
- `"text"`: narration or choice string
    - Every scene must have a narration string
    - Zero or more choices may then follow, each with a string and a target scene. If no choices are given, the story ends after the narration.
    - `[IF expr]`: conditionally show a choice if a given expression is true
        - Expressions can use variables, literals, and basic operators (`=` for equality, `!=` for inequality, `>` and `<` for comparisons)
    - `[THEN expr]`: run a side effect when a choice is taken
- `{var}`: interpolate a variable into text
