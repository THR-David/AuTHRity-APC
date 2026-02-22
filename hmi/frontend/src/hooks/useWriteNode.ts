import { useCallback, useRef } from 'react';
import { useTagStore } from '../store/tagStore';

interface WriteNodeOptions {
  timeout?: number; // milliseconds, default 3000
  onSuccess?: () => void;
  onError?: (error: Error) => void;
}

export const useWriteNode = (
  nodeId: string,
  wsRef: React.RefObject<WebSocket | null>,
  options: WriteNodeOptions = {}
) => {
  const { timeout = 3000, onSuccess, onError } = options;
  const { setPending, clearPending, isPending } = useTagStore();
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  
  const isWriting = useTagStore(state => state.pendingWrites.has(nodeId));
  
  const write = useCallback((value: number | string | boolean) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      const error = new Error('WebSocket not connected');
      onError?.(error);
      return;
    }
    
    // Mark as pending
    setPending(nodeId);
    
    // Send write command
    const payload = {
      type: "WRITE",
      nodeId: nodeId,
      value: typeof value === 'number' ? Number(value) : value
    };
    
    wsRef.current.send(JSON.stringify(payload));
    
    // Set timeout for write acknowledgment
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }
    
    timeoutRef.current = setTimeout(() => {
      if (isPending(nodeId)) {
        clearPending(nodeId);
        const error = new Error(`Write timeout for ${nodeId}`);
        console.error('⏱️ Write Timeout:', nodeId);
        onError?.(error);
      }
    }, timeout);
    
    // Success handler (called when tag updates and pending is cleared)
    // This is handled automatically by the store's updateTag action
    const checkInterval = setInterval(() => {
      if (!isPending(nodeId)) {
        clearInterval(checkInterval);
        if (timeoutRef.current) {
          clearTimeout(timeoutRef.current);
          timeoutRef.current = null;
        }
        console.log('✅ Write Acknowledged:', nodeId);
        onSuccess?.();
      }
    }, 100);
    
    // Cleanup check after timeout + 500ms
    setTimeout(() => {
      clearInterval(checkInterval);
    }, timeout + 500);
    
  }, [nodeId, wsRef, setPending, clearPending, isPending, timeout, onSuccess, onError]);
  
  return {
    isWriting,
    write
  };
};
