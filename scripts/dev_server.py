# cors_server.py
# Local development server with CORS headers for the Pyodide web interface.
#
# Usage:
#   python scripts/dev_server.py
#
# This serves files from the current directory on port 8000 with CORS enabled,
# allowing the Pyodide web interface to load the local .whl package during
# development. Access the web interface at http://localhost:8000/web/.
from http.server import HTTPServer, SimpleHTTPRequestHandler


class CORSRequestHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        super().end_headers()


HTTPServer(("", 8000), CORSRequestHandler).serve_forever()
