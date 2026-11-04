FROM python:3.13-slim-bookworm

# Create a non-root user and group
RUN groupadd -r appgroup && useradd -m -r -g appgroup appuser

RUN apt-get update && apt-get install -y --no-install-recommends \
    wget \
    curl \
    git \
    unzip \
    # For additional fonts needed, specifically Chinese
    texlive-fonts-recommended \
    # For cython
    gcc \
    # For ebook-convert HTML to ePub
    xz-utils \
    libopengl0 \
    libegl1 \
    libopengl0 \
    libxcb-cursor0 \
    libxcb-xinerama0 \
    libxkbcommon0 \
    libglx0 \
    libnss3 \
    libdeflate0 \
    # For weasyprint
    pango1.0-tools \
    # For fc-cache
    fontconfig \
    # For princexml
    fonts-khmeros \
    fonts-lklug-sinhala \
    fonts-tlwg-garuda-otf \
    fonts-lohit-orya \
    fonts-lohit-mlym \
    fonts-lohit-knda \
    fonts-lohit-telu \
    fonts-lohit-taml \
    fonts-lohit-gujr \
    fonts-lohit-guru \
    fonts-lohit-beng-bengali \
    fonts-lohit-deva \
    fonts-noto \
    # fonts-noto-core \
    # fonts-noto-unhinted \
    fonts-noto-cjk \
    fonts-dzongkha \
    fonts-tibetan-machine \
    fonts-baekmuk \
    fonts-ipafont-mincho \
    fonts-arphic-uming \
    fonts-opensymbol \
    fonts-liberation2 \
    libaom3 \
    libavif15 \
    libgif7 \
    libjpeg62-turbo \
    liblcms2-2 \
    libtiff6 \
    libwebp7 \
    libwebpdemux2 \
 && fc-cache -f -v

# Download and install the PrinceXML .deb file
RUN wget https://www.princexml.com/download/prince_16.1-1_debian12_amd64.deb \
    && dpkg -i prince_16.1-1_debian12_amd64.deb || apt-get install -fy

# Get and install needed fonts.
RUN cd /tmp \
    && git clone --depth 1 https://github.com/Bible-Translation-Tools/ScriptureAppBuilder-pipeline \
    && cp /tmp/ScriptureAppBuilder-pipeline/ContainerImage/home/fonts/*.ttf /usr/share/fonts/
# Refresh system font cache.
RUN fc-cache -f -v


# Get and install calibre for use of its ebook-convert binary for HTML to ePub conversion.
RUN wget -nv -O- https://download.calibre-ebook.com/linux-installer.sh | sh /dev/stdin install_dir=/home/appuser/calibre-bin isolated=y


# Add calibre to PATH
ENV PATH="/home/appuser/calibre-bin/calibre:${PATH}"

# Test the installation
RUN ebook-convert --version

WORKDIR /app


# Make the output directory where resource asset files are cloned.
RUN mkdir -p assets_download
# Make the directory where intermediate document parts are saved.
RUN mkdir -p working_temp
# Make the output directory where generated HTML and PDFs are placed.
RUN mkdir -p document_output
# Make the directory where stet source documents are stored
RUN mkdir -p stet
# Make the directory where passage source documents are stored
RUN mkdir -p passages


COPY backend/stet/data/stet_*.docx stet/
COPY backend/passages/data/Spiritual_Terms_Evaluation_Exhaustive_Verse_List.txt passages/

COPY pyproject.toml .
COPY ./backend/requirements.txt .
COPY ./backend/requirements-prod.txt .

# See https://pythonspeed.com/articles/activate-virtualenv-dockerfile/
# for why a Python virtual env is used inside Docker.
ENV VIRTUAL_ENV=/opt/venv
RUN python -m venv ${VIRTUAL_ENV}
ENV PATH=${VIRTUAL_ENV}/bin:${PATH}

RUN pip install -v --upgrade pip
RUN pip install -v -r requirements.txt
RUN pip install -v -r requirements-prod.txt

COPY ./backend ./backend
COPY ./tests ./tests
COPY .env .
COPY template.docx .
COPY template_compact.docx .

# Next two lines are useful when the data (graphql) API are down so
# that we can still test
# COPY resources.json assets_download/resources.json
# RUN touch assets_download/resources.json

# Make sure Python can find the code to run
ENV PYTHONPATH=/app/backend:/app/tests

# Inside the Python virtual env: install any missing mypy type packages and check types in strict mode.
RUN mypy --strict --install-types --non-interactive backend/doc/**/*.py
RUN mypy --strict --install-types --non-interactive backend/stet/**/*.py
RUN mypy --strict --install-types --non-interactive backend/passages/**/*.py
RUN mypy --strict --install-types --non-interactive tests/**/*.py

# Change ownership of app specific directories to the non-root user
RUN chown -R appuser:appgroup /app /home/appuser/calibre-bin

# Switch to the non-root user
USER appuser

# Expose necessary ports (if any)
EXPOSE 8000
