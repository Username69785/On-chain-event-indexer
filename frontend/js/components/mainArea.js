import { state, getters } from '../state.js';
import { showGlobalToast } from '../utils.js';

let liveMetricsIntervalId = null;

function formatElapsedTime(startedAt, finishedAt) {
    if (!startedAt) return '-';

    const elapsedMs = Math.max((finishedAt ?? Date.now()) - startedAt, 0);
    const totalSeconds = Math.floor(elapsedMs / 1000);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
        return `${hours}h ${minutes}m`;
    }

    if (minutes > 0) {
        return `${minutes}m ${seconds}s`;
    }

    return `${seconds}s`;
}

function formatSpeed(speedPerSecond, processedTransactions) {
    if (speedPerSecond > 0) {
        const roundedSpeed = speedPerSecond >= 10
            ? Math.round(speedPerSecond)
            : speedPerSecond.toFixed(1);

        return `${roundedSpeed} tx/s`;
    }

    return processedTransactions > 0 ? '0 tx/s' : '-';
}

function formatRemainingTime(remainingTransactions, speedPerSecond) {
    if (remainingTransactions <= 0) {
        return '0 min';
    }

    if (speedPerSecond <= 0) {
        return '-';
    }

    const remainingMinutes = Math.max(
        1,
        Math.ceil((remainingTransactions / speedPerSecond) / 60)
    );

    return `${remainingMinutes} min`;
}

function syncLiveMetricsTimer() {
    const currentItem = getters.getActiveItem();
    const shouldUpdateLiveMetrics = Boolean(
        currentItem &&
        currentItem.status === 'indexing' &&
        currentItem.indexingStartedAt &&
        !currentItem.finishedAt
    );

    if (shouldUpdateLiveMetrics && !liveMetricsIntervalId) {
        liveMetricsIntervalId = window.setInterval(() => {
            const el = document.getElementById('elapsedTimeValue');
            const item = getters.getActiveItem();
            if (el && item && item.status === 'indexing') {
                el.textContent = formatElapsedTime(item.indexingStartedAt, item.finishedAt);
            }
        }, 1000);
    }

    if (!shouldUpdateLiveMetrics && liveMetricsIntervalId) {
        window.clearInterval(liveMetricsIntervalId);
        liveMetricsIntervalId = null;
    }
}

export function renderMainArea() {
    const mainContent = document.getElementById('mainContent');
    if (!mainContent) return;
    syncLiveMetricsTimer();

    if (!state.activeAddress) {
        mainContent.innerHTML = `
            <div class="empty-state">
                <div class="empty-text">
                    <h3>No Address Selected</h3>
                    <p>Select an address from the sidebar or add a new one to start indexing on-chain events.</p>
                </div>
                <button id="emptyStateAddBtn" class="primary-btn mt-4">
                    <span>Add First Address</span>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M12 5V19M5 12H19" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>
            </div>
        `;
        
        const addBtn = document.getElementById('emptyStateAddBtn');
        if (addBtn) {
            addBtn.addEventListener('click', () => {
                const sidebarAddBtn = document.getElementById('addAddressBtn');
                if (sidebarAddBtn) sidebarAddBtn.click();
            });
        }
        return;
    }

    const currentItem = getters.getActiveItem();
    if (!currentItem) return;
    
    const shortAddr = `${currentItem.address.slice(0, 4)}...${currentItem.address.slice(-4)}`;
    const totalTransactions = currentItem.totalTransactions ?? 0;
    const processedTransactions = currentItem.processedTransactions ?? 0;
    const remainingTransactions = currentItem.remainingTransactions ?? 0;
    const speedPerSecond = currentItem.speedPerSecond ?? 0;
    
    const percent = totalTransactions > 0 ? Math.floor((processedTransactions / totalTransactions) * 100) : 0;
    const speedLabel = formatSpeed(speedPerSecond, processedTransactions);
    const elapsedLabel = formatElapsedTime(currentItem.indexingStartedAt, currentItem.finishedAt);
    const remainingTimeLabel = formatRemainingTime(remainingTransactions, speedPerSecond);

    mainContent.innerHTML = `
        <div class="active-header" style="display:flex; flex-direction:column; margin-bottom:32px;">
            <div style="display:flex; gap:16px; align-items:center; margin-bottom: 8px;">
                <h1 title="${currentItem.address}" style="font-size: 1.8rem; font-family: var(--font-mono); font-weight: 700; color: var(--text-primary); cursor: default;">
                    ${shortAddr}
                </h1>
                
                <!-- Action Buttons right next to title -->
                <div style="display:flex; gap:8px;">
                    <button class="action-btn" id="copyBtn" title="Copy Address">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M8 16H6C4.89543 16 4 15.1046 4 14V6C4 4.89543 4.89543 4 6 4H14C15.1046 4 16 4.89543 16 6V8M18 8H10C8.89543 8 8 8.89543 8 10V18C8 19.1046 8.89543 20 10 20H18C19.1046 20 20 19.1046 20 18V10C20 8.89543 19.1046 8 18 8Z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                    </button>
                    <a href="https://solscan.io/account/${currentItem.address}" target="_blank" class="action-btn" title="Open in Solscan">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M10 4H6C4.89543 4 4 4.89543 4 6V18C4 19.1046 4.89543 20 6 20H18C19.1046 20 20 19.1046 20 18V14M11 13L20 4M20 4H15M20 4V9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                    </a>
                </div>
            </div>

            <div style="display:flex; gap:8px; align-items:center;">
                <span class="status-indicator ${currentItem.status}"></span>
                <span style="color:var(--text-secondary); font-size:0.9rem; text-transform:capitalize;">${currentItem.status}</span>
            </div>
        </div>
        
        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 24px;">
            <!-- Left Card -->
            <div class="card" style="background: rgba(255, 255, 255, 0.02); border: 1px solid rgba(255, 255, 255, 0.04); border-radius: 16px; padding: 20px 24px; display: flex; flex-direction: column; height: 100%; transition: border-color 0.2s ease;">
                <!-- Top Level -->
                <div style="display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px;">
                    <h3 style="font-size: 13px; font-weight: 500; letter-spacing: 0.07em; color: #94A3B8; text-transform: uppercase; margin: 0;">Transactions Stats</h3>
                    <span style="font-size: 24px; font-weight: 600; font-family: var(--font-mono); color: #F8FAFC; line-height: 1;">${percent}%</span>
                </div>
                
                <!-- Middle Level (Progress bar) -->
                <div style="height: 4px; background: rgba(255, 255, 255, 0.05); border-radius: 4px; overflow: hidden; width: 100%; margin-bottom: 32px;">
                    <div style="height: 100%; box-shadow: 0 0 10px rgba(59, 130, 246, 0.5); background: var(--accent-primary); width: ${percent}%; border-radius: 4px; transition: width 0.4s cubic-bezier(0.4, 0, 0.2, 1);"></div>
                </div>

                <!-- Bottom Level (Stats) -->
                <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-top: auto;">
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${totalTransactions}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Total</span>
                    </div>
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${processedTransactions}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Processed</span>
                    </div>
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${remainingTransactions}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Remaining</span>
                    </div>
                </div>
            </div>

            <!-- Right Card: Indexing Details -->
            <div class="card" style="background: rgba(255, 255, 255, 0.02); border: 1px solid rgba(255, 255, 255, 0.04); border-radius: 16px; padding: 20px 24px; display: flex; flex-direction: column; height: 100%; transition: border-color 0.2s ease;">
                <!-- Top Level -->
                <div style="display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px;">
                    <h3 style="font-size: 13px; font-weight: 500; letter-spacing: 0.07em; color: #94A3B8; text-transform: uppercase; margin: 0;">Indexing Details</h3>
                    <span style="font-size: 24px; font-weight: 600; font-family: var(--font-mono); visibility: hidden; line-height: 1;">0%</span>
                </div>
                
                <!-- Invisible progress bar skeleton to preserve exact visual alignment -->
                <div style="height: 4px; width: 100%; margin-bottom: 32px;"></div>
                
                <!-- Bottom Level (Stats) -->
                <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-top: auto;">
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${speedLabel}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Speed</span>
                    </div>
                    
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span id="elapsedTimeValue" style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${elapsedLabel}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Elapsed</span>
                    </div>
                    
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <span style="font-family: var(--font-mono); font-size: 24px; font-weight: 600; color: #F8FAFC; line-height: 1;">${remainingTimeLabel}</span>
                        <span style="color: #94A3B8; font-size: 12px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.06em;">Remaining Time</span>
                    </div>
                </div>
            </div>
        </div>
    `;

    const copyBtn = document.getElementById('copyBtn');
    if (copyBtn) {
        copyBtn.addEventListener('click', () => {
            navigator.clipboard.writeText(currentItem.address)
                .then(() => showGlobalToast('Address copied!'))
                .catch(err => console.error('Copy failed', err));
        });
    }
}
